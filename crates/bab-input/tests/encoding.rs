//! What bytes reach the shell when you press a key or click.

use bab_input::key::Modifiers;
use bab_input::mouse::{MouseButton, MouseEvent, MouseEventKind};
use bab_input::{Key, keyboard, mouse};
use bab_vt::{Modes, MouseTracking};

fn modes() -> Modes {
    Modes::default()
}

fn key(key: Key, modifiers: Modifiers) -> Vec<u8> {
    keyboard::encode(&key, modifiers, &modes()).expect("key should encode")
}

// ---- characters ------------------------------------------------------------

#[test]
fn plain_characters_send_utf8() {
    assert_eq!(key(Key::Char('a'), Modifiers::NONE), b"a");
    assert_eq!(key(Key::Char('ব'), Modifiers::NONE), "ব".as_bytes());
}

#[test]
fn control_maps_letters_to_c0_codes() {
    assert_eq!(key(Key::Char('a'), Modifiers::CONTROL), vec![0x01]);
    assert_eq!(key(Key::Char('c'), Modifiers::CONTROL), vec![0x03]);
    // Case is irrelevant: ctrl-shift-c is still 0x03.
    assert_eq!(key(Key::Char('C'), Modifiers::CONTROL), vec![0x03]);
    assert_eq!(key(Key::Char(' '), Modifiers::CONTROL), vec![0x00]);
    assert_eq!(key(Key::Char('['), Modifiers::CONTROL), vec![0x1b]);
}

/// `ctrl-1` has no legacy encoding. Sending `1` would be wrong, so we send nothing.
#[test]
fn control_with_an_unmappable_character_sends_nothing() {
    assert!(keyboard::encode(&Key::Char('1'), Modifiers::CONTROL, &modes()).is_none());
}

#[test]
fn alt_prefixes_an_escape() {
    assert_eq!(key(Key::Char('b'), Modifiers::ALT), vec![0x1b, b'b']);
    assert_eq!(
        key(Key::Char('c'), Modifiers::ALT | Modifiers::CONTROL),
        vec![0x1b, 0x03]
    );
}

/// Super belongs to the window manager, not the application.
#[test]
fn super_swallows_the_key() {
    assert!(keyboard::encode(&Key::Char('c'), Modifiers::SUPER, &modes()).is_none());
}

// ---- named keys ------------------------------------------------------------

#[test]
fn named_keys_send_their_control_codes() {
    assert_eq!(key(Key::Enter, Modifiers::NONE), b"\r");
    assert_eq!(key(Key::Tab, Modifiers::NONE), b"\t");
    assert_eq!(key(Key::Escape, Modifiers::NONE), vec![0x1b]);
    assert_eq!(key(Key::Backspace, Modifiers::NONE), vec![0x7f]);
}

#[test]
fn shift_tab_sends_a_back_tab() {
    assert_eq!(key(Key::Tab, Modifiers::SHIFT), b"\x1b[Z");
}

#[test]
fn control_backspace_sends_a_literal_backspace() {
    assert_eq!(key(Key::Backspace, Modifiers::CONTROL), vec![0x08]);
}

// ---- cursor keys -----------------------------------------------------------

#[test]
fn arrows_send_csi_by_default() {
    assert_eq!(key(Key::Up, Modifiers::NONE), b"\x1b[A");
    assert_eq!(key(Key::Left, Modifiers::NONE), b"\x1b[D");
    assert_eq!(key(Key::Home, Modifiers::NONE), b"\x1b[H");
}

/// `DECCKM` swaps the introducer, which is why full-screen apps set it.
#[test]
fn application_cursor_keys_send_ss3() {
    let modes = Modes {
        application_cursor_keys: true,
        ..Modes::default()
    };

    assert_eq!(
        keyboard::encode(&Key::Up, Modifiers::NONE, &modes).unwrap(),
        b"\x1bOA"
    );
}

/// A modified arrow always uses `CSI`, even in application mode. Emitting `SS3` with a
/// parameter would be malformed and applications would misread it.
#[test]
fn a_modified_arrow_stays_csi_in_application_mode() {
    let modes = Modes {
        application_cursor_keys: true,
        ..Modes::default()
    };

    let bytes = keyboard::encode(&Key::Up, Modifiers::CONTROL, &modes).unwrap();
    assert_eq!(bytes, b"\x1b[1;5A");
}

#[test]
fn modifiers_use_the_xterm_parameter() {
    // shift = 2, alt = 3, ctrl = 5, ctrl+shift = 6.
    assert_eq!(key(Key::Up, Modifiers::SHIFT), b"\x1b[1;2A");
    assert_eq!(key(Key::Up, Modifiers::ALT), b"\x1b[1;3A");
    assert_eq!(key(Key::Up, Modifiers::CONTROL), b"\x1b[1;5A");
    assert_eq!(
        key(Key::Up, Modifiers::CONTROL | Modifiers::SHIFT),
        b"\x1b[1;6A"
    );
}

// ---- editing and function keys ---------------------------------------------

#[test]
fn editing_keys_use_the_tilde_form() {
    assert_eq!(key(Key::Insert, Modifiers::NONE), b"\x1b[2~");
    assert_eq!(key(Key::Delete, Modifiers::NONE), b"\x1b[3~");
    assert_eq!(key(Key::PageUp, Modifiers::NONE), b"\x1b[5~");
    assert_eq!(key(Key::PageDown, Modifiers::CONTROL), b"\x1b[6;5~");
}

#[test]
fn function_keys_one_to_four_use_ss3() {
    assert_eq!(key(Key::Function(1), Modifiers::NONE), b"\x1bOP");
    assert_eq!(key(Key::Function(4), Modifiers::NONE), b"\x1bOS");
}

/// The numbering skips 16 and 22 for historical reasons, not by mistake.
#[test]
fn higher_function_keys_use_the_tilde_form_with_gaps() {
    assert_eq!(key(Key::Function(5), Modifiers::NONE), b"\x1b[15~");
    assert_eq!(key(Key::Function(6), Modifiers::NONE), b"\x1b[17~");
    assert_eq!(key(Key::Function(10), Modifiers::NONE), b"\x1b[21~");
    assert_eq!(key(Key::Function(11), Modifiers::NONE), b"\x1b[23~");
    assert_eq!(key(Key::Function(12), Modifiers::NONE), b"\x1b[24~");
}

#[test]
fn an_unknown_function_key_sends_nothing() {
    assert!(keyboard::encode(&Key::Function(13), Modifiers::NONE, &modes()).is_none());
}

// ---- mouse -----------------------------------------------------------------

fn click(row: usize, col: usize) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Press,
        button: Some(MouseButton::Left),
        row,
        col,
        modifiers: Modifiers::NONE,
    }
}

fn tracking(sgr: bool, tracking: MouseTracking) -> Modes {
    Modes {
        mouse_tracking: tracking,
        sgr_mouse: sgr,
        ..Modes::default()
    }
}

/// An application that never asked for mouse reports must not receive them.
#[test]
fn mouse_events_are_silent_until_requested() {
    assert!(mouse::encode(click(0, 0), &Modes::default()).is_none());
}

#[test]
fn sgr_mouse_reports_one_indexed_coordinates() {
    let modes = tracking(true, MouseTracking::Click);
    assert_eq!(mouse::encode(click(2, 4), &modes).unwrap(), b"\x1b[<0;5;3M");
}

#[test]
fn sgr_distinguishes_release_with_a_lowercase_final_byte() {
    let modes = tracking(true, MouseTracking::Click);
    let mut event = click(0, 0);
    event.kind = MouseEventKind::Release;

    assert_eq!(mouse::encode(event, &modes).unwrap(), b"\x1b[<0;1;1m");
}

#[test]
fn mouse_modifiers_are_added_to_the_button_field() {
    let modes = tracking(true, MouseTracking::Click);
    let mut event = click(0, 0);
    event.modifiers = Modifiers::CONTROL;

    assert_eq!(mouse::encode(event, &modes).unwrap(), b"\x1b[<16;1;1M");
}

#[test]
fn wheel_events_use_the_high_button_codes() {
    let modes = tracking(true, MouseTracking::Click);
    let mut event = click(0, 0);
    event.button = Some(MouseButton::WheelUp);

    assert_eq!(mouse::encode(event, &modes).unwrap(), b"\x1b[<64;1;1M");
}

/// Click tracking reports presses only; drag tracking adds motion with a button held.
#[test]
fn tracking_mode_filters_motion() {
    let mut motion = click(0, 0);
    motion.kind = MouseEventKind::Motion;

    assert!(mouse::encode(motion, &tracking(true, MouseTracking::Click)).is_none());
    assert!(mouse::encode(motion, &tracking(true, MouseTracking::Drag)).is_some());

    let mut hover = motion;
    hover.button = None;
    assert!(mouse::encode(hover, &tracking(true, MouseTracking::Drag)).is_none());
    assert!(mouse::encode(hover, &tracking(true, MouseTracking::Motion)).is_some());
}

#[test]
fn legacy_x10_encoding_offsets_by_32() {
    let modes = tracking(false, MouseTracking::Click);
    assert_eq!(
        mouse::encode(click(0, 0), &modes).unwrap(),
        vec![0x1b, b'[', b'M', 32, 33, 33]
    );
}

/// X10 packs a coordinate into one byte, so it cannot address a wide window. Reporting
/// the wrong cell would be worse than reporting nothing.
#[test]
fn legacy_x10_drops_coordinates_it_cannot_express() {
    let modes = tracking(false, MouseTracking::Click);
    assert!(mouse::encode(click(0, 300), &modes).is_none());
    // SGR has no such limit.
    assert!(mouse::encode(click(0, 300), &tracking(true, MouseTracking::Click)).is_some());
}
