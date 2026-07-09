//! Turning a glyph id into a coverage bitmap.

use bab_text::Face;
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

/// An 8-bit coverage bitmap and where it sits relative to the pen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct GlyphBitmap {
    pub width: u32,
    pub height: u32,
    /// Offset from the pen to the left edge of the bitmap.
    pub left: i32,
    /// Offset from the baseline up to the top edge of the bitmap.
    pub top: i32,
    /// One byte of coverage per pixel, row-major.
    pub coverage: Vec<u8>,
}

impl GlyphBitmap {
    /// Whitespace rasterizes to nothing. There is no point storing it in an atlas.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Rasterizes glyphs with `swash`.
///
/// Holds a [`ScaleContext`], which caches scaling state between calls, so it is worth
/// keeping one around rather than constructing per glyph.
#[derive(Default)]
pub struct Rasterizer {
    context: ScaleContext,
}

impl std::fmt::Debug for Rasterizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rasterizer").finish_non_exhaustive()
    }
}

impl Rasterizer {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rasterize `glyph_id` from `face` at `size_px`.
    ///
    /// Returns `None` when the glyph has no outline, which is the normal case for
    /// space and other blank glyphs.
    pub fn rasterize(&mut self, face: &Face, glyph_id: u16, size_px: f32) -> Option<GlyphBitmap> {
        let font = FontRef::from_index(face.bytes(), 0)?;
        let mut scaler = self.context.builder(font).size(size_px).hint(true).build();

        // Outline first, then embedded bitmaps and colour strikes as a fallback, which
        // is what emoji fonts carry.
        let image = Render::new(&[
            Source::ColorOutline(0),
            Source::ColorBitmap(StrikeWith::BestFit),
            Source::Outline,
        ])
        .format(Format::Alpha)
        .render(&mut scaler, glyph_id)?;

        let bitmap = GlyphBitmap {
            width: image.placement.width,
            height: image.placement.height,
            left: image.placement.left,
            top: image.placement.top,
            coverage: image.data,
        };

        (!bitmap.is_empty()).then_some(bitmap)
    }
}
