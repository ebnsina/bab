//! GPU glyph renderer for the terminal grid.
//!
//! One pipeline draws everything. Glyphs sample a coverage atlas; cell backgrounds
//! sample a reserved opaque texel in the same atlas, so there is no branch and no
//! second pipeline.

pub mod atlas;
pub mod palette;
pub mod raster;
pub mod renderer;

pub use atlas::{Atlas, AtlasEntry, GlyphKey};
pub use palette::Palette;
pub use raster::{GlyphBitmap, Rasterizer};
pub use renderer::{GridMetrics, Renderer};
