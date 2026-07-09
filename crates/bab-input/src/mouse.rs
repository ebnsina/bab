//! Encoding mouse events.
//!
//! Two encodings, chosen by what the application asked for. `SGR` (`?1006`) is the one
//! to prefer: the legacy `X10` form packs coordinates into single bytes and silently
//! breaks past column 223, which any real terminal window exceeds.

use bab_vt::{Modes, MouseTracking};

use crate::key::Modifiers;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseEventKind {
    Press,
    Release,
    /// Movement, with a button held or not.
    Motion,
}

/// A mouse event in cell coordinates, zero-indexed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub button: Option<MouseButton>,
    pub row: usize,
    pub col: usize,
    pub modifiers: Modifiers,
}

/// Largest coordinate the legacy `X10` encoding can express: `255 - 32`.
const X10_LIMIT: usize = 223;

impl MouseButton {
    /// The low bits of the button field.
    const fn code(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Middle => 1,
            Self::Right => 2,
            Self::WheelUp => 64,
            Self::WheelDown => 65,
        }
    }

    const fn is_wheel(self) -> bool {
        matches!(self, Self::WheelUp | Self::WheelDown)
    }
}

/// Encode a mouse event, or `None` when the application does not want it.
#[must_use]
pub fn encode(event: MouseEvent, modes: &Modes) -> Option<Vec<u8>> {
    if !wants_event(event, modes) {
        return None;
    }

    let button = button_field(event);
    // Reported coordinates are one-indexed.
    let (col, row) = (event.col + 1, event.row + 1);

    if modes.sgr_mouse {
        let final_byte = if event.kind == MouseEventKind::Release {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{button};{col};{row}{final_byte}").into_bytes());
    }

    // X10 cannot express a release button, nor coordinates past its limit. Dropping the
    // event is better than reporting a click on the wrong cell.
    if col > X10_LIMIT || row > X10_LIMIT {
        return None;
    }
    let button = if event.kind == MouseEventKind::Release {
        3
    } else {
        button
    };
    Some(vec![
        0x1b,
        b'[',
        b'M',
        32 + button,
        32 + col as u8,
        32 + row as u8,
    ])
}

/// Whether the application's tracking mode covers this event.
fn wants_event(event: MouseEvent, modes: &Modes) -> bool {
    match modes.mouse_tracking {
        MouseTracking::Off => false,
        MouseTracking::Click => event.kind != MouseEventKind::Motion,
        // Drag reports motion only while a button is held.
        MouseTracking::Drag => event.kind != MouseEventKind::Motion || event.button.is_some(),
        MouseTracking::Motion => true,
    }
}

fn button_field(event: MouseEvent) -> u8 {
    let mut field = match event.button {
        Some(button) => button.code(),
        // Motion with no button held reports as button 3, "released".
        None => 3,
    };

    // Wheel events never report motion, so the motion bit is safe to add otherwise.
    if event.kind == MouseEventKind::Motion && !event.button.is_some_and(MouseButton::is_wheel) {
        field += 32;
    }

    if event.modifiers.contains(Modifiers::SHIFT) {
        field += 4;
    }
    if event.modifiers.contains(Modifiers::ALT) {
        field += 8;
    }
    if event.modifiers.contains(Modifiers::CONTROL) {
        field += 16;
    }
    field
}
