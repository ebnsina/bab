//! Selecting text by cell coordinates.
//!
//! Selection is a UI concept laid over the grid. It never touches terminal state, and
//! it addresses cells, not codepoints — a cluster is selected or it is not, so a
//! conjunct can never be cut in half.

use crate::cell::Cell;
use crate::grid::{Cursor, Grid};

/// Characters that end a word for double-click selection.
///
/// Path separators are deliberately absent: double-clicking a path should select the
/// whole path, which is what you almost always want in a terminal.
const WORD_SEPARATORS: &[char] = &[
    ' ', '\t', '"', '\'', '`', '(', ')', '[', ']', '{', '}', '<', '>', '|', ';', ':', ',', '=',
];

/// What a click selects.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SelectionMode {
    /// One click and drag: cell by cell.
    #[default]
    Cell,
    /// Two clicks: whole words.
    Word,
    /// Three clicks: whole lines.
    Line,
}

impl SelectionMode {
    /// The mode a click with this count selects. Four clicks wrap back to one.
    #[must_use]
    pub const fn from_click_count(clicks: u32) -> Self {
        match clicks % 3 {
            2 => Self::Word,
            0 => Self::Line,
            _ => Self::Cell,
        }
    }
}

/// A range of cells, anchored where the drag began.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Selection {
    anchor: Cursor,
    head: Cursor,
    mode: SelectionMode,
}

impl Selection {
    #[must_use]
    pub const fn new(anchor: Cursor, mode: SelectionMode) -> Self {
        Self {
            anchor,
            head: anchor,
            mode,
        }
    }

    /// Move the dragging end. The anchor stays where the drag began, so dragging back
    /// past the start inverts the selection rather than emptying it.
    pub const fn drag_to(&mut self, head: Cursor) {
        self.head = head;
    }

    #[must_use]
    pub const fn mode(&self) -> SelectionMode {
        self.mode
    }

    /// The start and end, in reading order.
    #[must_use]
    pub fn ordered(&self) -> (Cursor, Cursor) {
        let backwards = (self.head.row, self.head.col) < (self.anchor.row, self.anchor.col);
        if backwards {
            (self.head, self.anchor)
        } else {
            (self.anchor, self.head)
        }
    }

    /// A selection that covers no cell. A click without a drag selects nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mode == SelectionMode::Cell && self.anchor == self.head
    }

    /// The range this selection covers on `grid`, expanded for word and line modes.
    #[must_use]
    pub fn range(&self, grid: &Grid) -> (Cursor, Cursor) {
        let (mut start, mut end) = self.ordered();

        match self.mode {
            SelectionMode::Cell => {}
            SelectionMode::Word => {
                start = word_start(grid, start);
                end = word_end(grid, end);
            }
            SelectionMode::Line => {
                start.col = 0;
                end.col = grid.cols().saturating_sub(1);
            }
        }
        (start, end)
    }

    /// Whether `row`, `col` falls inside the selection.
    #[must_use]
    pub fn contains(&self, grid: &Grid, row: usize, col: usize) -> bool {
        if self.is_empty() {
            return false;
        }
        let (start, end) = self.range(grid);
        if row < start.row || row > end.row {
            return false;
        }
        let after_start = row > start.row || col >= start.col;
        let before_end = row < end.row || col <= end.col;
        after_start && before_end
    }

    /// The selected text, with a newline between rows.
    ///
    /// Trailing blanks are trimmed from each row: a terminal pads lines with spaces it
    /// never meant as content, and pasting them back is never what anyone wanted.
    #[must_use]
    pub fn text(&self, grid: &Grid) -> String {
        if self.is_empty() {
            return String::new();
        }
        let (start, end) = self.range(grid);
        let mut out = String::new();

        for row in start.row..=end.row.min(grid.rows().saturating_sub(1)) {
            let first = if row == start.row { start.col } else { 0 };
            let last = if row == end.row {
                end.col
            } else {
                grid.cols() - 1
            };

            let mut line = String::new();
            for col in first..=last.min(grid.cols() - 1) {
                if let Some(cluster) = grid.cell(row, col).and_then(Cell::cluster) {
                    line.push_str(cluster.text());
                }
            }

            if row > start.row {
                out.push('\n');
            }
            out.push_str(line.trim_end());
        }
        out
    }
}

fn cell_char(grid: &Grid, row: usize, col: usize) -> Option<char> {
    grid.cell(row, col)?.cluster()?.text().chars().next()
}

fn is_separator(grid: &Grid, row: usize, col: usize) -> bool {
    match cell_char(grid, row, col) {
        Some(c) => WORD_SEPARATORS.contains(&c),
        // An empty cell is a gap, which is a boundary.
        None => true,
    }
}

fn word_start(grid: &Grid, at: Cursor) -> Cursor {
    let mut col = at.col;
    while col > 0 && !is_separator(grid, at.row, col - 1) {
        col -= 1;
    }
    Cursor { row: at.row, col }
}

fn word_end(grid: &Grid, at: Cursor) -> Cursor {
    let mut col = at.col;
    let last = grid.cols().saturating_sub(1);
    while col < last && !is_separator(grid, at.row, col + 1) {
        col += 1;
    }
    Cursor { row: at.row, col }
}
