//! An offscreen GPU renderer for a terminal grid.

use std::collections::HashMap;

use anyhow::{Context, Result};
use bab_text::{CellMetrics, FontStack, HarfRustShaper, ShapedCluster, Shaper, place, to_px};
use bab_vt::{Cursor, CursorShape, CursorStyle, Grid};
use bytemuck::{Pod, Zeroable};

use crate::atlas::{Atlas, GlyphKey};
use crate::palette::Palette;
use crate::raster::Rasterizer;
use crate::target::{Frame, Target};

const ATLAS_SIZE: u32 = 2048;
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

/// Thickness of a bar or underline cursor, in pixels.
const CURSOR_THICKNESS: f32 = 2.0;
/// Thickness of the outline drawn when the terminal is not focused.
const CURSOR_OUTLINE: f32 = 1.0;

/// Where and how to draw the cursor.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CursorState {
    pub position: Cursor,
    pub style: CursorStyle,
    /// An unfocused terminal draws a hollow box, whatever the shape.
    pub focused: bool,
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

    target: Target,
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
        Self::build(width, height, fonts, font_size, |_, _, device| {
            Ok(Target::offscreen(device, width, height))
        })
    }

    /// Build a renderer drawing into a `CAMetalLayer`.
    ///
    /// # Safety
    ///
    /// `layer` must be a valid, retained `CAMetalLayer` that outlives the renderer.
    #[cfg(target_os = "macos")]
    pub unsafe fn new_for_metal_layer(
        layer: *mut std::ffi::c_void,
        width: u32,
        height: u32,
        fonts: FontStack,
        font_size: f32,
    ) -> Result<Self> {
        Self::build(
            width,
            height,
            fonts,
            font_size,
            |instance, adapter, device| {
                // SAFETY: the caller guarantees the layer is valid and outlives us.
                let surface = unsafe {
                    instance
                        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
                }
                .context("failed to create a surface from the layer")?;
                Target::surface(surface, adapter, device, width, height)
            },
        )
    }

    fn build(
        width: u32,
        height: u32,
        fonts: FontStack,
        font_size: f32,
        make_target: impl FnOnce(&wgpu::Instance, &wgpu::Adapter, &wgpu::Device) -> Result<Target>,
    ) -> Result<Self> {
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

        let target = make_target(&instance, &adapter, &device)?;

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

        let pipeline = create_pipeline(&device, &layout, target.format());

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

    /// Change the font size, in physical pixels, and remeasure the cell.
    ///
    /// The shaping cache survives: shaped positions are in font design units and do
    /// not depend on size. Atlas entries are keyed by size, so nothing is invalidated.
    pub fn set_font_size(&mut self, size_px: f32) -> Result<()> {
        if (size_px - self.font_size).abs() < f32::EPSILON {
            return Ok(());
        }
        self.metrics = GridMetrics::measure(&self.fonts, size_px)?;
        self.font_size = size_px;
        Ok(())
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

        self.target.resize(&self.device, width, height);
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

    /// Draw `grid` into the target, with an optional cursor. Presents when windowed.
    pub fn render(&mut self, grid: &Grid, cursor: Option<CursorState>) -> Result<()> {
        let instances = self.build_instances(grid, cursor)?;
        self.upload_instances(&instances);

        let frame = self.target.acquire(&self.device)?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let background = self.palette.background;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bab pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: frame.view(),
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
        if let Frame::Surface { texture, .. } = frame {
            self.queue.present(texture);
        }
        Ok(())
    }

    /// Build the quads for one frame: backgrounds, then the cursor, then glyphs.
    ///
    /// The cursor sits under the glyphs so that a filled block does not hide the
    /// character it covers — the glyph is redrawn in the background colour instead.
    fn build_instances(
        &mut self,
        grid: &Grid,
        cursor: Option<CursorState>,
    ) -> Result<Vec<Instance>> {
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
        let mut cursor_quads = Vec::new();
        let mut glyphs = Vec::new();

        // A focused block cursor covers the cell, so the glyph under it must be
        // repainted in the background colour or it disappears.
        let inverted_cell =
            cursor.filter(|cursor| cursor.focused && cursor.style.shape == CursorShape::Block);

        if let Some(cursor) = cursor {
            cursor_quads = cursor_instances(cursor, cell, palette.foreground, atlas.solid_uv());
        }

        for row in 0..grid.rows() {
            for col in 0..grid.cols() {
                let Some(cell_data) = grid.cell(row, col) else {
                    continue;
                };
                let (mut fg, bg) = palette.colors_for(cell_data.attrs);

                if inverted_cell.is_some_and(|c| c.position.row == row && c.position.col == col) {
                    fg = palette.background;
                }

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

        backgrounds.append(&mut cursor_quads);
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
    ///
    /// Only an offscreen target can be read back; a presented surface texture is gone.
    pub fn read_pixels(&self) -> Result<Vec<u8>> {
        let Target::Offscreen { texture, .. } = &self.target else {
            anyhow::bail!("only an offscreen renderer can read pixels back");
        };
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
                texture,
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

/// The quads for one cursor: a filled shape when focused, a hollow box when not.
fn cursor_instances(
    cursor: CursorState,
    cell: CellMetrics,
    color: [f32; 4],
    uv: [f32; 4],
) -> Vec<Instance> {
    let x = cursor.position.col as f32 * cell.width;
    let y = cursor.position.row as f32 * cell.height;

    let quad = |rect: [f32; 4]| Instance { rect, uv, color };

    if !cursor.focused {
        // A hollow box, drawn as four edges. An unfocused terminal must not look
        // like it will accept the next keystroke.
        let t = CURSOR_OUTLINE;
        return vec![
            quad([x, y, cell.width, t]),
            quad([x, y + cell.height - t, cell.width, t]),
            quad([x, y, t, cell.height]),
            quad([x + cell.width - t, y, t, cell.height]),
        ];
    }

    vec![match cursor.style.shape {
        CursorShape::Block => quad([x, y, cell.width, cell.height]),
        CursorShape::Underline => quad([
            x,
            y + cell.height - CURSOR_THICKNESS,
            cell.width,
            CURSOR_THICKNESS,
        ]),
        CursorShape::Bar => quad([x, y, CURSOR_THICKNESS, cell.height]),
    }]
}

fn create_instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bab instances"),
        size: (capacity * std::mem::size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
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
                format,
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
