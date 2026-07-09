//! Pseudoterminal process management, and the session that binds it to a terminal.

pub mod pty;
pub mod session;

pub use pty::{Command, Pty, Size};
pub use session::Session;
