//! Scrollback, alternate screen, scroll regions, editing, and resize.

use bab_vt::{CellContent, MouseTracking, Terminal};

fn render(bytes: &[u8]) -> Terminal {
    let mut term = Terminal::new(4, 20);
    term.feed(bytes);
    term
}

// ---- scrollback ------------------------------------------------------------

#[test]
fn lines_scrolled_off_the_top_enter_scrollback() {
    let mut term = Terminal::new(2, 10);
    term.feed(b"one\r\ntwo\r\nthree");

    assert_eq!(term.grid().scrollback().len(), 1);
    assert_eq!(term.grid().scrollback_text(0).unwrap(), "one");
    assert_eq!(term.grid().row_text(0), "two");
    assert_eq!(term.grid().row_text(1), "three");
}

/// The alternate screen has no scrollback; a full-screen app must not pollute it.
#[test]
fn alt_screen_does_not_accumulate_scrollback() {
    let mut term = Terminal::new(2, 10);
    term.feed(b"\x1b[?1049h");
    term.feed(b"one\r\ntwo\r\nthree");

    assert!(term.grid().scrollback().is_empty());
}

// ---- alternate screen ------------------------------------------------------

#[test]
fn alt_screen_preserves_the_primary_screen() {
    let mut term = Terminal::new(3, 10);
    term.feed(b"primary");
    term.feed(b"\x1b[?1049h");
    assert_eq!(term.grid().row_text(0), "");

    term.feed(b"alternate");
    assert_eq!(term.grid().row_text(0), "alternate");

    term.feed(b"\x1b[?1049l");
    assert_eq!(term.grid().row_text(0), "primary");
}

#[test]
fn alt_screen_restores_the_cursor_on_exit() {
    let mut term = Terminal::new(4, 20);
    term.feed(b"\x1b[3;7H");
    term.feed(b"\x1b[?1049h");
    term.feed(b"\x1b[1;1Hx");
    term.feed(b"\x1b[?1049l");

    let cursor = term.grid().cursor();
    assert_eq!((cursor.row, cursor.col), (2, 6));
}

#[test]
fn alt_screen_is_cleared_on_entry() {
    let mut term = Terminal::new(3, 10);
    term.feed(b"\x1b[?1049h");
    term.feed(b"stale");
    term.feed(b"\x1b[?1049l");
    term.feed(b"\x1b[?1049h");

    assert_eq!(term.grid().row_text(0), "");
}

// ---- scroll region ---------------------------------------------------------

#[test]
fn scroll_region_confines_scrolling() {
    let mut term = Terminal::new(4, 10);
    term.feed(b"a\r\nb\r\nc\r\nd");
    // Region covers rows 2-3 (one-indexed), i.e. "b" and "c".
    term.feed(b"\x1b[2;3r");
    term.feed(b"\x1b[3;1H");
    term.feed(b"\n\n");

    assert_eq!(term.grid().row_text(0), "a");
    assert_eq!(term.grid().row_text(3), "d");
}

/// Content scrolled out of a region that does not touch row 0 is discarded,
/// not pushed into scrollback.
#[test]
fn scrolling_inside_a_region_does_not_reach_scrollback() {
    let mut term = Terminal::new(4, 10);
    term.feed(b"a\r\nb\r\nc\r\nd");
    term.feed(b"\x1b[2;3r\x1b[3;1H\n");

    assert!(term.grid().scrollback().is_empty());
}

#[test]
fn reverse_index_scrolls_down_at_the_top_margin() {
    let mut term = Terminal::new(3, 10);
    term.feed(b"a\r\nb\r\nc");
    term.feed(b"\x1b[1;1H");
    term.feed(b"\x1bM");

    assert_eq!(term.grid().row_text(0), "");
    assert_eq!(term.grid().row_text(1), "a");
}

// ---- line and character editing --------------------------------------------

#[test]
fn insert_and_delete_lines() {
    let mut term = Terminal::new(3, 10);
    term.feed(b"a\r\nb\r\nc");
    term.feed(b"\x1b[2;1H\x1b[L");
    assert_eq!(term.grid().row_text(1), "");
    assert_eq!(term.grid().row_text(2), "b");

    term.feed(b"\x1b[2;1H\x1b[M");
    assert_eq!(term.grid().row_text(1), "b");
}

#[test]
fn insert_and_delete_chars() {
    let mut term = Terminal::new(2, 10);
    term.feed(b"abcd\x1b[1;2H\x1b[2@");
    assert_eq!(term.grid().row_text(0), "abcd");

    let mut term = Terminal::new(2, 10);
    term.feed(b"abcd\x1b[1;2H\x1b[2P");
    assert_eq!(term.grid().row_text(0), "ad");
}

#[test]
fn erase_chars_blanks_without_shifting() {
    let mut term = Terminal::new(2, 10);
    term.feed(b"abcd\x1b[1;2H\x1b[2X");
    assert_eq!(term.grid().row_text(0), "ad");
    assert_eq!(
        term.grid().cell(0, 3).unwrap().cluster().unwrap().text(),
        "d"
    );
}

/// Shifting cells apart must not leave half a wide cluster behind.
#[test]
fn deleting_into_a_wide_cluster_erases_all_of_it() {
    let mut term = Terminal::new(2, 10);
    term.feed("世a".as_bytes());
    term.feed(b"\x1b[1;1H\x1b[1P");

    let grid = term.grid();
    assert_eq!(grid.row_text(0), "a");
    assert_eq!(grid.cell(0, 0).unwrap().content, CellContent::Empty);
}

#[test]
fn inserting_into_a_wide_cluster_erases_all_of_it() {
    let mut term = Terminal::new(2, 10);
    term.feed("世a".as_bytes());
    term.feed(b"\x1b[1;2H\x1b[1@");

    assert_eq!(term.grid().row_text(0), "a");
}

// ---- resize ----------------------------------------------------------------

#[test]
fn resize_preserves_content() {
    let mut term = Terminal::new(3, 10);
    term.feed(b"hello\r\nworld");
    term.resize(5, 20);

    assert_eq!(term.grid().rows(), 5);
    assert_eq!(term.grid().cols(), 20);
    assert_eq!(term.grid().row_text(0), "hello");
    assert_eq!(term.grid().row_text(1), "world");
}

/// Narrowing truncates a cluster that no longer fits, rather than rendering a fragment.
#[test]
fn narrowing_erases_a_truncated_wide_cluster() {
    let mut term = Terminal::new(2, 10);
    term.feed("ab世".as_bytes());
    term.resize(2, 3);

    assert_eq!(term.grid().row_text(0), "ab");
    assert_eq!(term.grid().cell(0, 2).unwrap().content, CellContent::Empty);
}

#[test]
fn shrinking_rows_clamps_the_cursor() {
    let mut term = Terminal::new(5, 10);
    term.feed(b"\x1b[5;3H");
    term.resize(2, 10);

    assert!(term.grid().cursor().row < 2);
}

// ---- queries ---------------------------------------------------------------

#[test]
fn device_status_report_returns_the_cursor() {
    let mut term = render(b"\x1b[3;5H\x1b[6n");
    assert_eq!(term.take_output(), b"\x1b[3;5R");
}

#[test]
fn device_attributes_identifies_the_terminal() {
    let mut term = render(b"\x1b[c");
    assert_eq!(term.take_output(), b"\x1b[?62;22c");
}

#[test]
fn output_is_drained_once() {
    let mut term = render(b"\x1b[c");
    assert!(!term.take_output().is_empty());
    assert!(term.take_output().is_empty());
}

// ---- modes -----------------------------------------------------------------

#[test]
fn private_modes_toggle() {
    let mut term = render(b"\x1b[?2004h\x1b[?25l\x1b[?2026h\x1b[?2031h");
    let modes = term.modes();
    assert!(modes.bracketed_paste);
    assert!(!modes.cursor_visible);
    assert!(modes.synchronized_output);
    assert!(modes.color_scheme_updates);

    term.feed(b"\x1b[?2004l");
    assert!(!term.modes().bracketed_paste);
}

#[test]
fn mouse_tracking_modes() {
    let mut term = render(b"\x1b[?1002h\x1b[?1006h");
    assert_eq!(term.modes().mouse_tracking, MouseTracking::Drag);
    assert!(term.modes().sgr_mouse);

    term.feed(b"\x1b[?1002l");
    assert_eq!(term.modes().mouse_tracking, MouseTracking::Off);
}

#[test]
fn autowrap_off_pins_to_the_last_column() {
    let mut term = Terminal::new(3, 4);
    term.feed(b"\x1b[?7l");
    term.feed(b"abcdef");

    assert_eq!(term.grid().row_text(1), "");
    assert_eq!(
        term.grid().cell(0, 3).unwrap().cluster().unwrap().text(),
        "f"
    );
}

#[test]
fn save_and_restore_cursor() {
    let term = render(b"\x1b[2;3H\x1b7\x1b[4;9H\x1b8");
    let cursor = term.grid().cursor();
    assert_eq!((cursor.row, cursor.col), (1, 2));

    let term = render(b"\x1b[2;3H\x1b[s\x1b[4;9H\x1b[u");
    let cursor = term.grid().cursor();
    assert_eq!((cursor.row, cursor.col), (1, 2));
}

/// `DECSC` saves rendition, not just position.
#[test]
fn restore_cursor_restores_attributes() {
    use bab_vt::Flags;
    let term = render(b"\x1b[1m\x1b7\x1b[0m\x1b8x");
    assert!(
        term.grid()
            .cell(0, 0)
            .unwrap()
            .attrs
            .flags
            .contains(Flags::BOLD)
    );
}
