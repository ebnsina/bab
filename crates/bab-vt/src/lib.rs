//! Grapheme-cluster-aware terminal grid and VT state machine.
//!
//! The cell is the unit of layout, and a cell holds one grapheme cluster. Cluster
//! width comes from `wcwidth`, matching what TUI applications compute — never from
//! the shaper. See `docs/adr/0001-width-contract.md`.

pub mod attrs;
pub mod cell;
pub mod grid;
pub mod modes;
pub mod sgr;
pub mod term;
pub mod width;

pub use attrs::{Attrs, Color, Flags};
pub use cell::{Cell, CellContent, Cluster};
pub use grid::{Cursor, Grid, LineErase, SavedCursor, ScreenErase};
pub use modes::{CursorShape, CursorStyle, Mode, Modes, MouseTracking};
pub use term::Terminal;
pub use width::{char_cells, cluster_cells};
