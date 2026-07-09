//! Shaping a grapheme cluster into positioned glyphs.

use anyhow::Result;
use harfrust::{ShapeOptions, UnicodeBuffer};

use crate::face::Face;

/// One positioned glyph, in font design units.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShapedGlyph {
    pub glyph_id: u16,
    pub x_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}

impl ShapedGlyph {
    /// The `.notdef` glyph, drawn as tofu. Its presence means the face lacked a glyph.
    #[must_use]
    pub const fn is_notdef(&self) -> bool {
        self.glyph_id == 0
    }

    /// A glyph that advances nothing and hangs off its base.
    ///
    /// Bengali conjuncts are drawn this way: a base consonant plus below-forms and
    /// reph, each with zero advance and a negative offset. Counting glyphs therefore
    /// tells you nothing about how wide a cluster is — count advances instead.
    #[must_use]
    pub const fn is_mark(&self) -> bool {
        self.x_advance == 0
    }
}

/// The result of shaping one grapheme cluster, in font design units.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ShapedCluster {
    pub glyphs: Vec<ShapedGlyph>,
    /// Total advance the shaper wants. This is *not* the cell span — see `layout`.
    pub advance: i32,
    /// Index into the [`FontStack`](crate::FontStack) that shaped this cluster.
    pub face_index: usize,
}

impl ShapedCluster {
    /// Whether any glyph is missing, meaning the chosen face could not draw this text.
    #[must_use]
    pub fn has_tofu(&self) -> bool {
        self.glyphs.iter().any(ShapedGlyph::is_notdef)
    }

    /// Glyphs that contribute width. Marks and below-forms do not.
    #[must_use]
    pub fn advancing_glyphs(&self) -> usize {
        self.glyphs.iter().filter(|glyph| !glyph.is_mark()).count()
    }

    /// The advance in pixels at `size_px`, given the face's units per em.
    #[must_use]
    pub fn advance_px(&self, units_per_em: u16, size_px: f32) -> f32 {
        crate::layout::to_px(self.advance, units_per_em, size_px)
    }
}

/// Turns text into positioned glyphs.
///
/// Behind a trait so the engine can be swapped. `harfrust` is a pure-Rust port of
/// HarfBuzz from the HarfBuzz organisation; if its Indic coverage ever proves short
/// of upstream, a `harfbuzz-sys` backend drops in without touching callers.
pub trait Shaper: std::fmt::Debug + Send + Sync {
    /// Shape one grapheme cluster with `face`.
    fn shape(&self, cluster: &str, face: &Face, face_index: usize) -> Result<ShapedCluster>;
}

/// Shaping via `harfrust`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HarfRustShaper;

impl Shaper for HarfRustShaper {
    fn shape(&self, cluster: &str, face: &Face, face_index: usize) -> Result<ShapedCluster> {
        let font = face.font()?;
        let shaper = face.shaper_data().shaper(&font).build();

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(cluster);
        // Infers script, language, and direction from the text. Bengali resolves to
        // the Indic shaper, which is what reorders matras and forms conjuncts.
        buffer.guess_segment_properties();

        // No scale set, so positions come back in font design units.
        let shaped = shaper.shape(buffer, ShapeOptions::new());

        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();

        let mut glyphs = Vec::with_capacity(infos.len());
        let mut advance = 0_i32;
        for (info, position) in infos.iter().zip(positions) {
            advance += position.x_advance;
            glyphs.push(ShapedGlyph {
                // harfrust documents this as always within `u16`.
                glyph_id: u16::try_from(info.glyph_id).unwrap_or(0),
                x_advance: position.x_advance,
                x_offset: position.x_offset,
                y_offset: position.y_offset,
            });
        }

        Ok(ShapedCluster {
            glyphs,
            advance,
            face_index,
        })
    }
}
