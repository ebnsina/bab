//! The width contract: cluster segmentation must agree with `wcwidth`.
//!
//! These tests encode `docs/adr/0001-width-contract.md`. If one fails, either the
//! grid disagrees with what a TUI application computed, or a cluster was split.

use bab_vt::{CellContent, Terminal};
use unicode_width::UnicodeWidthStr;

/// Bengali: ব + hasant + ল forms one conjunct glyph.
const BANGLA_CONJUNCT: &str = "ব্ল";
/// Bengali: "bangla", five codepoints.
const BANGLA_WORD: &str = "বাংলা";

fn render(text: &str) -> Terminal {
    let mut term = Terminal::new(4, 20);
    term.feed(text.as_bytes());
    term
}

fn cluster_texts(term: &Terminal, row: usize) -> Vec<String> {
    term.grid()
        .clusters(row)
        .map(|c| c.text().to_owned())
        .collect()
}

#[test]
fn bangla_conjunct_is_one_cluster() {
    let term = render(BANGLA_CONJUNCT);
    assert_eq!(cluster_texts(&term, 0), vec![BANGLA_CONJUNCT]);
}

#[test]
fn bangla_word_round_trips() {
    let term = render(BANGLA_WORD);
    assert_eq!(term.grid().row_text(0), BANGLA_WORD);
}

/// The load-bearing invariant. The cells the grid allocates must equal the cells a
/// TUI application allocated with `wcwidth` — otherwise the cursor drifts.
#[test]
fn allocated_width_matches_wcwidth() {
    for text in [BANGLA_CONJUNCT, BANGLA_WORD, "hello", "世界", "héllo", "🇧🇩"] {
        let term = render(text);
        let allocated: usize = term
            .grid()
            .clusters(0)
            .map(|c| usize::from(c.width()))
            .sum();
        assert_eq!(allocated, text.width(), "width mismatch for {text:?}");
    }
}

#[test]
fn combining_mark_joins_its_base() {
    // "e" + U+0301 COMBINING ACUTE ACCENT
    let term = render("e\u{301}");
    assert_eq!(cluster_texts(&term, 0), vec!["e\u{301}"]);
    assert_eq!(term.grid().cursor().col, 1);
}

/// A combining mark with no base has no width. xterm discards it; so do we.
#[test]
fn orphan_combining_mark_is_discarded() {
    let term = render("\u{301}");
    assert_eq!(term.grid().row_text(0), "");
    assert_eq!(term.grid().cursor().col, 0);
}

#[test]
fn wide_char_spans_two_cells() {
    let term = render("世a");
    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).unwrap().cluster().unwrap().width(), 2);
    assert_eq!(grid.cell(0, 1).unwrap().content, CellContent::Continuation);
    assert_eq!(grid.cell(0, 2).unwrap().cluster().unwrap().text(), "a");
    assert_eq!(grid.cursor().col, 3);
}

#[test]
fn zwj_emoji_is_one_cluster() {
    // woman technologist: U+1F469 ZWJ U+1F4BB
    let term = render("\u{1F469}\u{200D}\u{1F4BB}");
    assert_eq!(cluster_texts(&term, 0).len(), 1);
}

/// Overwriting half of a wide cluster must erase all of it, or the leftover
/// continuation cell renders as a stale glyph fragment.
#[test]
fn overwriting_wide_cluster_clears_continuation() {
    let mut term = Terminal::new(2, 10);
    term.feed("世".as_bytes());
    term.feed(b"\r");
    term.feed(b"x");

    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).unwrap().cluster().unwrap().text(), "x");
    assert_eq!(grid.cell(0, 1).unwrap().content, CellContent::Empty);
}

/// Overwriting the *tail* of a wide cluster must erase its head too.
#[test]
fn overwriting_continuation_clears_head() {
    let mut term = Terminal::new(2, 10);
    term.feed("世".as_bytes());
    term.feed(b"\r\x1b[2C");
    term.feed(b"\x1b[1D");
    term.feed(b"x");

    let grid = term.grid();
    assert_eq!(grid.cell(0, 0).unwrap().content, CellContent::Empty);
    assert_eq!(grid.cell(0, 1).unwrap().cluster().unwrap().text(), "x");
}

#[test]
fn cursor_movement_breaks_the_open_cluster() {
    // A combining mark after a cursor move must not retroactively join the old base.
    let mut term = Terminal::new(2, 10);
    term.feed(b"e");
    term.feed(b"\x1b[1C");
    term.feed("\u{301}".as_bytes());
    assert_eq!(cluster_texts(&term, 0), vec!["e"]);
}
