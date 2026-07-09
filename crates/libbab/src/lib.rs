//! C ABI over the `bab` terminal core.
//!
//! The core owns the terminal; the host owns the window. This is the seam, and it is a
//! flat C interface on purpose: one header serves AppKit, GTK4, and WinUI3, rather
//! than three language-specific binding generators.

pub mod ffi;
pub mod terminal;

pub use terminal::Terminal;
