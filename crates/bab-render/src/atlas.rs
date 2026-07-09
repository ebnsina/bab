//! A shelf-packed glyph atlas backed by a single-channel GPU texture.

use std::collections::HashMap;

use anyhow::{Result, bail};
use etagere::{BucketedAtlasAllocator, size2};

use crate::raster::GlyphBitmap;

/// Identifies a rasterized glyph. Size is quantised so that a font size expressed as
/// a float does not produce a new atlas entry per redraw.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GlyphKey {
    pub face_index: usize,
    pub glyph_id: u16,
    /// Font size in 1/64 px, quantised.
    size: u32,
}

impl GlyphKey {
    #[must_use]
    pub fn new(face_index: usize, glyph_id: u16, size_px: f32) -> Self {
        Self {
            face_index,
            glyph_id,
            size: (size_px * 64.0).round() as u32,
        }
    }
}

/// Where a glyph lives in the atlas, in normalized texture coordinates.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AtlasEntry {
    pub uv: [f32; 4],
    pub width: f32,
    pub height: f32,
    pub left: i32,
    pub top: i32,
}

/// A growable-in-principle, fixed-in-practice glyph atlas.
///
/// The first texel is reserved as fully opaque so that solid quads — cell backgrounds,
/// the cursor — can sample the same texture as glyphs and share one pipeline.
pub struct Atlas {
    allocator: BucketedAtlasAllocator,
    entries: HashMap<GlyphKey, Option<AtlasEntry>>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: u32,
    solid_uv: [f32; 4],
}

impl std::fmt::Debug for Atlas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Atlas")
            .field("size", &self.size)
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}

impl Atlas {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, size: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bab glyph atlas"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut atlas = Self {
            allocator: BucketedAtlasAllocator::new(size2(size as i32, size as i32)),
            entries: HashMap::new(),
            texture,
            view,
            size,
            solid_uv: [0.0; 4],
        };
        atlas.solid_uv = atlas.reserve_solid_texel(queue);
        atlas
    }

    /// Reserve one fully opaque texel so solid quads can sample the glyph texture.
    ///
    /// The allocator does not hand out the origin first, so the texel's coordinates
    /// must be read back rather than assumed. All four are the texel centre, which
    /// keeps the sampler from ever reading a neighbour.
    fn reserve_solid_texel(&mut self, queue: &wgpu::Queue) -> [f32; 4] {
        let allocation = self
            .allocator
            .allocate(size2(1, 1))
            .expect("a fresh atlas must have room for one texel");

        let x = allocation.rectangle.min.x as u32;
        let y = allocation.rectangle.min.y as u32;
        self.upload(queue, x, y, 1, 1, &[u8::MAX]);

        let centre_u = (x as f32 + 0.5) / self.size as f32;
        let centre_v = (y as f32 + 0.5) / self.size as f32;
        [centre_u, centre_v, centre_u, centre_v]
    }

    #[must_use]
    pub const fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Normalized coordinates of the reserved opaque texel.
    #[must_use]
    pub const fn solid_uv(&self) -> [f32; 4] {
        self.solid_uv
    }

    /// Look up a glyph, rasterizing and uploading it on first use.
    ///
    /// `Ok(None)` means the glyph has no ink — a space, say — and should not be drawn.
    pub fn entry(
        &mut self,
        queue: &wgpu::Queue,
        key: GlyphKey,
        rasterize: impl FnOnce() -> Option<GlyphBitmap>,
    ) -> Result<Option<AtlasEntry>> {
        if let Some(cached) = self.entries.get(&key) {
            return Ok(*cached);
        }

        let Some(bitmap) = rasterize() else {
            self.entries.insert(key, None);
            return Ok(None);
        };

        let entry = self.insert(queue, &bitmap)?;
        self.entries.insert(key, Some(entry));
        Ok(Some(entry))
    }

    fn insert(&mut self, queue: &wgpu::Queue, bitmap: &GlyphBitmap) -> Result<AtlasEntry> {
        // A one-texel gutter stops neighbouring glyphs bleeding into each other when
        // the sampler filters, and costs nothing at this scale.
        let padded = size2(bitmap.width as i32 + 1, bitmap.height as i32 + 1);
        let Some(allocation) = self.allocator.allocate(padded) else {
            bail!("glyph atlas is full");
        };

        let x = allocation.rectangle.min.x as u32;
        let y = allocation.rectangle.min.y as u32;
        self.upload(queue, x, y, bitmap.width, bitmap.height, &bitmap.coverage);

        let scale = self.size as f32;
        Ok(AtlasEntry {
            uv: [
                x as f32 / scale,
                y as f32 / scale,
                (x + bitmap.width) as f32 / scale,
                (y + bitmap.height) as f32 / scale,
            ],
            width: bitmap.width as f32,
            height: bitmap.height as f32,
            left: bitmap.left,
            top: bitmap.top,
        })
    }

    fn upload(&self, queue: &wgpu::Queue, x: u32, y: u32, width: u32, height: u32, data: &[u8]) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }
}
