//! Selecting text by cell, word, and line.

use bab_vt::{Cursor, Selection, SelectionMode, Terminal};

fn term(text: &str) -> Terminal {
    let mut term = Terminal::new(4, 20);
    term.feed(text.as_bytes());
    term
}

fn at(row: usize, col: usize) -> Cursor {
    Cursor { row, col }
}

fn select(anchor: Cursor, head: Cursor, mode: SelectionMode) -> Selection {
    let mut selection = Selection::new(anchor, mode);
    selection.drag_to(head);
    selection
}

#[test]
fn a_click_without_a_drag_selects_nothing() {
    let term = term("hello");
    let selection = Selection::new(at(0, 2), SelectionMode::Cell);
    assert!(selection.is_empty());
    assert_eq!(selection.text(term.grid()), "");
}

#[test]
fn dragging_selects_a_span_of_cells() {
    let term = term("hello world");
    let selection = select(at(0, 0), at(0, 4), SelectionMode::Cell);
    assert_eq!(selection.text(term.grid()), "hello");
}

/// Dragging back past the anchor inverts the range rather than emptying it.
#[test]
fn dragging_backwards_selects_the_same_text() {
    let term = term("hello world");
    let forwards = select(at(0, 0), at(0, 4), SelectionMode::Cell);
    let backwards = select(at(0, 4), at(0, 0), SelectionMode::Cell);
    assert_eq!(forwards.text(term.grid()), backwards.text(term.grid()));
}

#[test]
fn selection_spans_rows_with_a_newline() {
    let term = term("one\r\ntwo");
    let selection = select(at(0, 0), at(1, 2), SelectionMode::Cell);
    assert_eq!(selection.text(term.grid()), "one\ntwo");
}

/// A terminal pads lines with spaces it never meant as content.
#[test]
fn trailing_blanks_are_trimmed_from_each_row() {
    let term = term("hi\r\nthere");
    let selection = select(at(0, 0), at(1, 19), SelectionMode::Cell);
    assert_eq!(selection.text(term.grid()), "hi\nthere");
}

// ---- word and line modes ---------------------------------------------------

#[test]
fn double_click_selects_a_word() {
    let term = term("hello world");
    let selection = Selection::new(at(0, 7), SelectionMode::Word);
    assert_eq!(selection.text(term.grid()), "world");
}

/// A path is one word. Selecting only a fragment of it is never what anyone wanted.
#[test]
fn a_path_is_one_word() {
    let term = term("cat /usr/local/bin");
    let selection = Selection::new(at(0, 8), SelectionMode::Word);
    assert_eq!(selection.text(term.grid()), "/usr/local/bin");
}

#[test]
fn triple_click_selects_the_line() {
    let term = term("hello world\r\nsecond");
    let selection = Selection::new(at(0, 3), SelectionMode::Line);
    assert_eq!(selection.text(term.grid()), "hello world");
}

#[test]
fn click_count_maps_to_a_mode() {
    assert_eq!(SelectionMode::from_click_count(1), SelectionMode::Cell);
    assert_eq!(SelectionMode::from_click_count(2), SelectionMode::Word);
    assert_eq!(SelectionMode::from_click_count(3), SelectionMode::Line);
    // A fourth click starts over rather than doing nothing.
    assert_eq!(SelectionMode::from_click_count(4), SelectionMode::Cell);
}

// ---- clusters --------------------------------------------------------------

/// Selection addresses cells, so a conjunct can never be cut in half.
#[test]
fn a_wide_cluster_is_selected_whole() {
    let term = term("世界");
    // Column 1 is the continuation cell of the first character.
    let selection = select(at(0, 0), at(0, 1), SelectionMode::Cell);
    assert_eq!(selection.text(term.grid()), "世");
}

#[test]
fn a_bengali_cluster_survives_selection() {
    let term = term("ব্ল");
    let selection = select(at(0, 0), at(0, 1), SelectionMode::Cell);
    assert_eq!(selection.text(term.grid()), "ব্ল");
}

#[test]
fn contains_reports_membership() {
    let term = term("hello");
    let selection = select(at(0, 1), at(0, 3), SelectionMode::Cell);
    assert!(!selection.contains(term.grid(), 0, 0));
    assert!(selection.contains(term.grid(), 0, 2));
    assert!(!selection.contains(term.grid(), 0, 4));
}
