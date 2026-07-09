//! Complex-script shaping and font fallback for a cell grid.
//!
//! Shaping decides what glyphs to draw. It never decides how many cells they occupy —
//! the grid already did that with `wcwidth`, matching the application on the other end
//! of the pty. See `docs/adr/0001-width-contract.md`.

pub mod face;
pub mod layout;
pub mod shaper;

pub use face::{Face, FaceMetrics, FontStack};
pub use layout::{CellMetrics, to_px};
pub use shaper::{HarfRustShaper, ShapedCluster, ShapedGlyph, Shaper};
