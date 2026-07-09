//! Escape-sequence handling: cursor control, erasing, SGR, OSC.

use bab_vt::{Color, Flags, Terminal};

fn render(bytes: &[u8]) -> Terminal {
    let mut term = Terminal::new(4, 20);
    term.feed(bytes);
    term
}

#[test]
fn cup_positions_the_cursor_one_indexed() {
    let term = render(b"\x1b[2;5H");
    let cursor = term.grid().cursor();
    assert_eq!((cursor.row, cursor.col), (1, 4));
}

#[test]
fn cup_without_params_homes_the_cursor() {
    let term = render(b"abc\x1b[H");
    let cursor = term.grid().cursor();
    assert_eq!((cursor.row, cursor.col), (0, 0));
}

#[test]
fn newline_and_carriage_return() {
    let term = render(b"ab\r\nc");
    assert_eq!(term.grid().row_text(0), "ab");
    assert_eq!(term.grid().row_text(1), "c");
}

#[test]
fn line_wraps_at_the_right_edge() {
    let mut term = Terminal::new(3, 4);
    term.feed(b"abcde");
    assert_eq!(term.grid().row_text(0), "abcd");
    assert_eq!(term.grid().row_text(1), "e");
}

/// A double-width glyph must not straddle the right edge; it wraps whole.
#[test]
fn wide_char_wraps_rather_than_splitting() {
    let mut term = Terminal::new(3, 3);
    term.feed("ab世".as_bytes());
    assert_eq!(term.grid().row_text(0), "ab");
    assert_eq!(term.grid().row_text(1), "世");
}

#[test]
fn scroll_up_on_linefeed_at_the_bottom() {
    let mut term = Terminal::new(2, 10);
    term.feed(b"one\r\ntwo\r\nthree");
    assert_eq!(term.grid().row_text(0), "two");
    assert_eq!(term.grid().row_text(1), "three");
}

#[test]
fn erase_line_to_end() {
    let term = render(b"abcdef\x1b[1;4H\x1b[K");
    assert_eq!(term.grid().row_text(0), "abc");
}

#[test]
fn erase_screen_all() {
    let term = render(b"abc\r\ndef\x1b[2J");
    assert_eq!(term.grid().row_text(0), "");
    assert_eq!(term.grid().row_text(1), "");
}

#[test]
fn sgr_sets_and_resets_flags() {
    let term = render(b"\x1b[1;3ma");
    let attrs = term.grid().cell(0, 0).unwrap().attrs;
    assert!(attrs.flags.contains(Flags::BOLD));
    assert!(attrs.flags.contains(Flags::ITALIC));

    let term = render(b"\x1b[1m\x1b[0ma");
    assert!(
        !term
            .grid()
            .cell(0, 0)
            .unwrap()
            .attrs
            .flags
            .contains(Flags::BOLD)
    );
}

#[test]
fn sgr_named_colors() {
    let term = render(b"\x1b[31;42ma");
    let attrs = term.grid().cell(0, 0).unwrap().attrs;
    assert_eq!(attrs.fg, Color::Indexed(1));
    assert_eq!(attrs.bg, Color::Indexed(2));
}

#[test]
fn sgr_bright_colors() {
    let term = render(b"\x1b[91ma");
    assert_eq!(term.grid().cell(0, 0).unwrap().attrs.fg, Color::Indexed(9));
}

#[test]
fn sgr_truecolor_legacy_semicolon_form() {
    let term = render(b"\x1b[38;2;10;20;30ma");
    assert_eq!(
        term.grid().cell(0, 0).unwrap().attrs.fg,
        Color::Rgb(10, 20, 30)
    );
}

#[test]
fn sgr_truecolor_subparam_form() {
    let term = render(b"\x1b[38:2::10:20:30ma");
    assert_eq!(
        term.grid().cell(0, 0).unwrap().attrs.fg,
        Color::Rgb(10, 20, 30)
    );
}

#[test]
fn sgr_indexed_256_both_forms() {
    let term = render(b"\x1b[38;5;200ma");
    assert_eq!(
        term.grid().cell(0, 0).unwrap().attrs.fg,
        Color::Indexed(200)
    );

    let term = render(b"\x1b[38:5:200ma");
    assert_eq!(
        term.grid().cell(0, 0).unwrap().attrs.fg,
        Color::Indexed(200)
    );
}

#[test]
fn osc_sets_the_title() {
    let term = render(b"\x1b]0;hello\x07");
    assert_eq!(term.title(), Some("hello"));

    let term = render(b"\x1b]2;bab\x1b\\");
    assert_eq!(term.title(), Some("bab"));
}

#[test]
fn tab_advances_to_the_next_stop() {
    let term = render(b"a\tb");
    assert_eq!(term.grid().cursor().col, 9);
}
