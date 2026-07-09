//! Placing a shaped cluster inside the cells the grid already allocated.
//!
//! This is where `docs/adr/0001-width-contract.md` is enforced. The grid decided how
//! many cells a cluster occupies, using `wcwidth`, exactly as the application did.
//! Shaping happens afterwards and may disagree about the natural advance. It does not
//! get a vote: the glyph is placed *within* the allocated span.

use crate::shaper::ShapedCluster;

/// The size of one cell, in pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

/// Where to draw a cluster, and how badly it fits.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Placement {
    /// Pixels to shift the cluster right from the left edge of its span.
    pub x_offset: f32,
    /// Unused pixels in the span. Negative when the glyph overhangs its cells.
    pub slack: f32,
}

impl Placement {
    /// Whether the cluster is wider than the cells the grid gave it.
    #[must_use]
    pub fn overhangs(&self) -> bool {
        self.slack < 0.0
    }
}

/// Convert font design units to pixels.
#[must_use]
pub fn to_px(units: i32, units_per_em: u16, size_px: f32) -> f32 {
    if units_per_em == 0 {
        return 0.0;
    }
    units as f32 * size_px / f32::from(units_per_em)
}

/// Centre a shaped cluster in the `span` cells the grid allocated.
///
/// The glyph is never scaled to fit. A Bengali conjunct drawn in a proportional
/// fallback face will rarely fill its cells exactly, and distorted text is worse
/// than loosely spaced text. Applications that want the slack back can declare a
/// true width with `OSC 66`.
#[must_use]
pub fn place(
    cluster: &ShapedCluster,
    units_per_em: u16,
    size_px: f32,
    span: u16,
    cell: CellMetrics,
) -> Placement {
    let advance = to_px(cluster.advance, units_per_em, size_px);
    let span_px = f32::from(span) * cell.width;
    let slack = span_px - advance;

    Placement {
        x_offset: slack / 2.0,
        slack,
    }
}
