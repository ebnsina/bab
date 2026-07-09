//! Scrolling the viewport back into history.

use bab_vt::Terminal;

/// Fill the scrollback with numbered lines, leaving `rows` on screen.
fn history(rows: usize, count: usize) -> Terminal {
    let mut term = Terminal::new(rows, 20);
    for i in 0..count {
        term.feed(format!("line{i}\r\n").as_bytes());
    }
    term
}

#[test]
fn the_viewport_starts_at_the_live_screen() {
    let term = history(3, 6);
    assert!(term.grid().is_at_bottom());
    assert_eq!(term.grid().viewport_offset(), 0);
}

#[test]
fn scrolling_back_shows_history() {
    let mut term = history(3, 6);
    let live = term.grid().display_row_text(0);

    term.grid_mut().scroll_back(2);
    assert_eq!(term.grid().viewport_offset(), 2);
    assert_ne!(term.grid().display_row_text(0), live);
    assert!(!term.grid().is_at_bottom());
}

#[test]
fn scrolling_back_then_forward_returns_to_the_live_screen() {
    let mut term = history(3, 6);
    let live = term.grid().display_row_text(0);

    term.grid_mut().scroll_back(3);
    term.grid_mut().scroll_forward(3);

    assert!(term.grid().is_at_bottom());
    assert_eq!(term.grid().display_row_text(0), live);
}

#[test]
fn scrolling_saturates_at_the_oldest_line() {
    let mut term = history(3, 6);
    let max = term.grid().max_scroll();
    term.grid_mut().scroll_back(9999);
    assert_eq!(term.grid().viewport_offset(), max);
}

#[test]
fn scrolling_forward_saturates_at_the_bottom() {
    let mut term = history(3, 6);
    term.grid_mut().scroll_forward(9999);
    assert!(term.grid().is_at_bottom());
}

/// The oldest line really is the oldest, not a copy of the screen.
#[test]
fn the_top_of_history_is_the_first_line_printed() {
    let mut term = history(3, 6);
    let max = term.grid().max_scroll();
    term.grid_mut().scroll_back(max);
    assert_eq!(term.grid().display_row_text(0), "line0");
}

/// A reader scrolled into history should not be dragged along by new output.
#[test]
fn new_output_does_not_move_a_scrolled_viewport() {
    let mut term = history(3, 6);
    term.grid_mut().scroll_back(2);
    let showing = term.grid().display_row_text(0);

    term.feed(b"fresh\r\n");

    assert_eq!(term.grid().display_row_text(0), showing);
    assert!(!term.grid().is_at_bottom());
}

/// The live screen keeps updating while the viewport is parked in history.
#[test]
fn the_live_screen_still_advances_while_scrolled() {
    let mut term = history(3, 6);
    term.grid_mut().scroll_back(2);
    term.feed(b"fresh");

    term.grid_mut().scroll_to_bottom();
    assert!(term.grid().display_row_text(2).contains("fresh"));
}

#[test]
fn display_cell_reads_history_and_screen() {
    let mut term = history(3, 6);
    term.grid_mut().scroll_back(1);

    // Every row of the viewport must resolve to a cell.
    for row in 0..term.grid().rows() {
        assert!(term.grid().display_cell(row, 0).is_some());
    }
}

/// The alternate screen keeps no history, so there is nowhere to scroll.
#[test]
fn the_alternate_screen_cannot_scroll_back() {
    let mut term = Terminal::new(3, 20);
    term.feed(b"\x1b[?1049h");
    term.feed(b"a\r\nb\r\nc\r\nd\r\n");
    assert_eq!(term.grid().max_scroll(), 0);
}
