//! An offscreen GPU renderer for a terminal grid.

use std::collections::HashMap;

use anyhow::{Context, Result};
use bab_text::{CellMetrics, FontStack, HarfRustShaper, ShapedCluster, Shaper, place, to_px};
use bab_vt::Grid;
use bytemuck::{Pod, Zeroable};

use crate::atlas::{Atlas, GlyphKey};
use crate::palette::Palette;
use crate::raster::Rasterizer;

const ATLAS_SIZE: u32 = 2048;
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// `copy_texture_to_buffer` requires each row to start on this boundary.
const COPY_ALIGNMENT: u32 = 256;

/// One quad: a glyph, or a solid fill sampling the atlas's opaque texel.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
struct Instance {
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    viewport: [f32; 2],
    _pad: [f32; 2],
}

/// The pixel geometry of one cell, derived from the primary face.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridMetrics {
    pub cell: CellMetrics,
    pub ascent: f32,
}

impl GridMetrics {
    /// Cell width comes from the primary face's advance for `0`, which is the
    /// monospace advance. Height comes from its vertical metrics.
    fn measure(fonts: &FontStack, size_px: f32) -> Result<Self> {
        let face = fonts.primary();
        let shaped = HarfRustShaper.shape("0", face, 0)?;
        let width = to_px(shaped.advance, face.units_per_em(), size_px);

        let metrics = face.metrics(size_px);
        let height = metrics.line_height().ceil().max(1.0);

        Ok(Self {
            cell: CellMetrics {
                width: width.ceil().max(1.0),
                height,
            },
            ascent: metrics.ascent,
        })
    }
}

/// Renders a [`Grid`] into an offscreen texture.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: usize,

    target: wgpu::Texture,
    target_view: wgpu::TextureView,
    width: u32,
    height: u32,

    atlas: Atlas,
    rasterizer: Rasterizer,
    shape_cache: HashMap<(usize, String), ShapedCluster>,

    fonts: FontStack,
    font_size: f32,
    metrics: GridMetrics,
    palette: Palette,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renderer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("metrics", &self.metrics)
            .finish_non_exhaustive()
    }
}

impl Renderer {
    /// Build a renderer drawing into a `width` by `height` offscreen texture.
    ///
    /// Fails when no GPU adapter is available, which is the normal situation in CI
    /// without a software rasterizer. Callers should skip rather than panic.
    pub fn new(width: u32, height: u32, fonts: FontStack, font_size: f32) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            ..Default::default()
        }))
        .context("no suitable GPU adapter")?;

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("bab device"),
            ..Default::default()
        }))
        .context("failed to request device")?;

        let metrics = GridMetrics::measure(&fonts, font_size)?;
        let atlas = Atlas::new(&device, &queue, ATLAS_SIZE);

        let (target, target_view) = create_target(&device, width, height);

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bab globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &globals,
            0,
            bytemuck::bytes_of(&Globals {
                viewport: [width as f32, height as f32],
                _pad: [0.0; 2],
            }),
        );

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bab atlas sampler"),
            // Nearest: glyphs are rasterized at their final size, so filtering would
            // only blur them and bleed across atlas neighbours.
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bab bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bab bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(atlas.view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline = create_pipeline(&device, &layout);

        let instance_capacity = 4096;
        let instances = create_instance_buffer(&device, instance_capacity);

        Ok(Self {
            device,
            queue,
            pipeline,
            bind_group,
            globals,
            instances,
            instance_capacity,
            target,
            target_view,
            width,
            height,
            atlas,
            rasterizer: Rasterizer::new(),
            shape_cache: HashMap::new(),
            fonts,
            font_size,
            metrics,
            palette: Palette::default(),
        })
    }

    #[must_use]
    pub const fn metrics(&self) -> GridMetrics {
        self.metrics
    }

    pub const fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
    }

    /// Resize the offscreen target.
    ///
    /// The viewport lives in the globals buffer, so it must be rewritten here or the
    /// vertex shader keeps projecting into the old size.
    pub fn resize(&mut self, width: u32, height: u32) {
        if (width, height) == (self.width, self.height) || width == 0 || height == 0 {
            return;
        }

        let (target, target_view) = create_target(&self.device, width, height);
        self.target = target;
        self.target_view = target_view;
        self.width = width;
        self.height = height;

        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                viewport: [width as f32, height as f32],
                _pad: [0.0; 2],
            }),
        );
    }

    /// The offscreen size needed to show `rows` by `cols` cells.
    #[must_use]
    pub fn pixel_size(&self, rows: usize, cols: usize) -> (u32, u32) {
        (
            (cols as f32 * self.metrics.cell.width).ceil() as u32,
            (rows as f32 * self.metrics.cell.height).ceil() as u32,
        )
    }

    /// Draw `grid` into the offscreen texture.
    pub fn render(&mut self, grid: &Grid) -> Result<()> {
        let instances = self.build_instances(grid)?;
        self.upload_instances(&instances);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let background = self.palette.background;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bab pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(background[0]),
                            g: f64::from(background[1]),
                            b: f64::from(background[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if !instances.is_empty() {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.instances.slice(..));
                pass.draw(0..6, 0..instances.len() as u32);
            }
        }

        self.queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Build the quads for one frame: cell backgrounds first, then glyphs over them.
    fn build_instances(&mut self, grid: &Grid) -> Result<Vec<Instance>> {
        // Destructured so the atlas and rasterizer can be borrowed at the same time.
        let Self {
            queue,
            atlas,
            rasterizer,
            shape_cache,
            fonts,
            font_size,
            metrics,
            palette,
            ..
        } = self;

        let cell = metrics.cell;
        let mut backgrounds = Vec::new();
        let mut glyphs = Vec::new();

        for row in 0..grid.rows() {
            for col in 0..grid.cols() {
                let Some(cell_data) = grid.cell(row, col) else {
                    continue;
                };
                let (fg, bg) = palette.colors_for(cell_data.attrs);

                if bg != palette.background {
                    backgrounds.push(Instance {
                        rect: [
                            col as f32 * cell.width,
                            row as f32 * cell.height,
                            cell.width,
                            cell.height,
                        ],
                        uv: atlas.solid_uv(),
                        color: bg,
                    });
                }

                let Some(cluster) = cell_data.cluster() else {
                    continue;
                };

                let (face_index, face) = fonts.resolve(cluster.text());
                let key = (face_index, cluster.text().to_owned());
                let shaped = match shape_cache.get(&key) {
                    Some(shaped) => shaped,
                    None => {
                        let shaped = HarfRustShaper.shape(cluster.text(), face, face_index)?;
                        shape_cache.entry(key).or_insert(shaped)
                    }
                };

                let upem = face.units_per_em();
                let placement = place(shaped, upem, *font_size, cluster.width(), cell);

                let mut pen_x = col as f32 * cell.width + placement.x_offset;
                let pen_y = row as f32 * cell.height + metrics.ascent;

                for glyph in &shaped.glyphs {
                    let entry = atlas.entry(
                        queue,
                        GlyphKey::new(face_index, glyph.glyph_id, *font_size),
                        || rasterizer.rasterize(face, glyph.glyph_id, *font_size),
                    )?;

                    if let Some(entry) = entry {
                        let x = pen_x + to_px(glyph.x_offset, upem, *font_size);
                        let y = pen_y - to_px(glyph.y_offset, upem, *font_size);
                        glyphs.push(Instance {
                            rect: [
                                x + entry.left as f32,
                                y - entry.top as f32,
                                entry.width,
                                entry.height,
                            ],
                            uv: entry.uv,
                            color: fg,
                        });
                    }

                    pen_x += to_px(glyph.x_advance, upem, *font_size);
                }
            }
        }

        backgrounds.append(&mut glyphs);
        Ok(backgrounds)
    }

    fn upload_instances(&mut self, instances: &[Instance]) {
        if instances.len() > self.instance_capacity {
            self.instance_capacity = instances.len().next_power_of_two();
            self.instances = create_instance_buffer(&self.device, self.instance_capacity);
        }
        if !instances.is_empty() {
            self.queue
                .write_buffer(&self.instances, 0, bytemuck::cast_slice(instances));
        }
    }

    /// Read the target back as tightly packed RGBA8, row-major.
    pub fn read_pixels(&self) -> Result<Vec<u8>> {
        let unpadded = self.width * 4;
        let padded = unpadded.div_ceil(COPY_ALIGNMENT) * COPY_ALIGNMENT;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bab readback"),
            size: u64::from(padded) * u64::from(self.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("waiting for readback")?;
        receiver.recv()??;

        let mapped = slice.get_mapped_range()?;
        let mut pixels = Vec::with_capacity((unpadded * self.height) as usize);
        for row in 0..self.height {
            let start = (row * padded) as usize;
            pixels.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();

        Ok(pixels)
    }
}

fn create_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bab target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bab instances"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pipeline(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bab shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bab pipeline layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bab pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
            })],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: TARGET_FORMAT,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
