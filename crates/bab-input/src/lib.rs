//! Keyboard and mouse encoding for a terminal.
//!
//! The platform shell turns an OS event into a [`Key`] or [`MouseEvent`]; this crate
//! turns that into the bytes the child process reads. Terminal modes decide the
//! encoding, so the current [`bab_vt::Modes`] is always an input.

pub mod key;
pub mod keyboard;
pub mod mouse;

pub use key::{Key, Modifiers};
pub use mouse::{MouseButton, MouseEvent, MouseEventKind};
