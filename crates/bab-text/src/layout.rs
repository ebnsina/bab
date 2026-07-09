//! Converting shaped positions to pixels.
//!
//! Layout within a row lives in the renderer, which shapes a whole run of cells and
//! lays its glyphs out contiguously from the run's first cell. See
//! `docs/adr/0001-width-contract.md`.

/// The size of one cell, in pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CellMetrics {
    pub width: f32,
    pub height: f32,
}

/// Convert font design units to pixels.
#[must_use]
pub fn to_px(units: i32, units_per_em: u16, size_px: f32) -> f32 {
    if units_per_em == 0 {
        return 0.0;
    }
    units as f32 * size_px / f32::from(units_per_em)
}
