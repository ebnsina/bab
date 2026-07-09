//! Encoding key presses as the bytes a terminal application expects.
//!
//! This is the legacy xterm encoding, which every application understands. The Kitty
//! keyboard protocol is a separate, negotiated layer and is not implemented yet: until
//! it is, keys that the legacy encoding cannot express — `ctrl-1`, or distinguishing
//! `ctrl-i` from `tab` — simply do not round-trip.

use bab_vt::Modes;

use crate::key::{Key, Modifiers};

/// Encode a key press. `None` means the key sends nothing.
#[must_use]
pub fn encode(key: &Key, modifiers: Modifiers, modes: &Modes) -> Option<Vec<u8>> {
    // Super is a window-manager modifier and never reaches the application.
    if modifiers.contains(Modifiers::SUPER) {
        return None;
    }

    let bytes = match key {
        Key::Char(c) => return encode_char(*c, modifiers),

        Key::Enter => vec![b'\r'],
        Key::Escape => vec![0x1b],
        Key::Backspace => vec![if modifiers.contains(Modifiers::CONTROL) {
            0x08
        } else {
            0x7f
        }],
        Key::Tab => {
            if modifiers.contains(Modifiers::SHIFT) {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }

        Key::Up => cursor_key(b'A', modifiers, modes),
        Key::Down => cursor_key(b'B', modifiers, modes),
        Key::Right => cursor_key(b'C', modifiers, modes),
        Key::Left => cursor_key(b'D', modifiers, modes),
        Key::Home => cursor_key(b'H', modifiers, modes),
        Key::End => cursor_key(b'F', modifiers, modes),

        Key::Insert => tilde_key(2, modifiers),
        Key::Delete => tilde_key(3, modifiers),
        Key::PageUp => tilde_key(5, modifiers),
        Key::PageDown => tilde_key(6, modifiers),

        Key::Function(n) => return encode_function(*n, modifiers),
    };

    // Alt on a named key is carried by the modifier parameter, not an escape prefix,
    // so only the plain forms need prefixing here. `cursor_key` and `tilde_key`
    // already emit the parameter when a modifier is held.
    Some(
        if modifiers == Modifiers::ALT && matches!(key, Key::Enter | Key::Escape | Key::Backspace) {
            prefix_escape(bytes)
        } else {
            bytes
        },
    )
}

/// Control and alt turn a character into something else entirely.
fn encode_char(c: char, modifiers: Modifiers) -> Option<Vec<u8>> {
    let mut bytes = if modifiers.contains(Modifiers::CONTROL) {
        vec![control_byte(c)?]
    } else {
        c.to_string().into_bytes()
    };

    if modifiers.contains(Modifiers::ALT) {
        bytes = prefix_escape(bytes);
    }
    Some(bytes)
}

/// The C0 control a character maps to when combined with ctrl.
///
/// Only these produce a control code. `ctrl-1` has no legacy encoding, which is one of
/// the gaps the Kitty keyboard protocol exists to close.
fn control_byte(c: char) -> Option<u8> {
    Some(match c {
        ' ' | '@' => 0x00,
        'a'..='z' => c as u8 - b'a' + 1,
        'A'..='Z' => c as u8 - b'A' + 1,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' => 0x1e,
        '_' => 0x1f,
        '?' => 0x7f,
        _ => return None,
    })
}

/// Arrows, Home, and End. `DECCKM` swaps `CSI` for `SS3`, but only unmodified.
fn cursor_key(final_byte: u8, modifiers: Modifiers, modes: &Modes) -> Vec<u8> {
    if modifiers.is_empty() {
        let introducer: &[u8] = if modes.application_cursor_keys {
            b"\x1bO"
        } else {
            b"\x1b["
        };
        return [introducer, &[final_byte]].concat();
    }
    format!("\x1b[1;{}{}", modifiers.xterm_param(), final_byte as char).into_bytes()
}

/// Keys encoded as `CSI n ~`, such as Delete and Page Up.
fn tilde_key(number: u8, modifiers: Modifiers) -> Vec<u8> {
    if modifiers.is_empty() {
        return format!("\x1b[{number}~").into_bytes();
    }
    format!("\x1b[{number};{}~", modifiers.xterm_param()).into_bytes()
}

/// `F1` to `F4` are `SS3`; the rest are `CSI n ~` with a non-obvious numbering.
fn encode_function(n: u8, modifiers: Modifiers) -> Option<Vec<u8>> {
    if (1..=4).contains(&n) {
        let final_byte = b'P' + (n - 1);
        if modifiers.is_empty() {
            return Some([b"\x1bO".as_slice(), &[final_byte]].concat());
        }
        return Some(
            format!("\x1b[1;{}{}", modifiers.xterm_param(), final_byte as char).into_bytes(),
        );
    }

    // The gaps at 16 and 22 are historical, not a mistake.
    let number = match n {
        5 => 15,
        6..=10 => n + 11,
        11 | 12 => n + 12,
        _ => return None,
    };
    Some(tilde_key(number, modifiers))
}

fn prefix_escape(bytes: Vec<u8>) -> Vec<u8> {
    let mut prefixed = Vec::with_capacity(bytes.len() + 1);
    prefixed.push(0x1b);
    prefixed.extend(bytes);
    prefixed
}
