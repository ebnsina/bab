//! The character grid: cursor, lines, and cluster placement.

use crate::attrs::Attrs;
use crate::cell::{Cell, CellContent, Cluster};

/// Cursor position, in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

/// Which part of a line an erase applies to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineErase {
    ToEnd,
    ToStart,
    All,
}

/// Which part of the screen an erase applies to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScreenErase {
    Below,
    Above,
    All,
}

/// A fixed-size grid of cells with a cursor.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grid {
    lines: Vec<Vec<Cell>>,
    cols: usize,
    rows: usize,
    cursor: Cursor,
    attrs: Attrs,
    /// Head of the cluster most recently printed, if the next character could still
    /// extend it. Any cursor movement or erase invalidates this.
    open_cluster: Option<Cursor>,
}

impl Grid {
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            lines: vec![vec![Cell::default(); cols]; rows],
            cols,
            rows,
            cursor: Cursor::default(),
            attrs: Attrs::default(),
            open_cluster: None,
        }
    }

    #[must_use]
    pub const fn rows(&self) -> usize {
        self.rows
    }

    #[must_use]
    pub const fn cols(&self) -> usize {
        self.cols
    }

    #[must_use]
    pub const fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub const fn attrs_mut(&mut self) -> &mut Attrs {
        &mut self.attrs
    }

    #[must_use]
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.lines.get(row)?.get(col)
    }

    /// The clusters on `row`, left to right.
    pub fn clusters(&self, row: usize) -> impl Iterator<Item = &Cluster> {
        self.lines
            .get(row)
            .into_iter()
            .flatten()
            .filter_map(Cell::cluster)
    }

    /// The text of `row` in logical order, with trailing blanks trimmed.
    #[must_use]
    pub fn row_text(&self, row: usize) -> String {
        self.clusters(row).map(Cluster::text).collect()
    }

    // ---- printing ----------------------------------------------------------

    /// Print `c`, either extending the open cluster or starting a new one.
    pub fn print(&mut self, c: char) {
        if let Some(open) = self.open_cluster
            && self.extend_open_cluster(open, c)
        {
            return;
        }
        self.place_new_cluster(c);
    }

    /// Try to absorb `c` into the cluster at `open`. Returns whether it was absorbed.
    fn extend_open_cluster(&mut self, open: Cursor, c: char) -> bool {
        let Some(CellContent::Head(cluster)) = self.content_at(open) else {
            return false;
        };

        let old_width = cluster.width();
        let mut candidate = cluster.clone();
        let Some(new_width) = candidate.try_extend(c) else {
            return false;
        };

        // A cluster that grew past the line edge keeps its old span. Absorbing the
        // codepoint anyway preserves it for copy and paste; dropping it would lose data.
        let fits = open.col + new_width as usize <= self.cols;
        if new_width == old_width || !fits {
            self.lines[open.row][open.col].content = CellContent::Head(candidate);
            return true;
        }

        self.write_cluster(open, candidate);
        self.cursor = Cursor {
            row: open.row,
            col: open.col + new_width as usize,
        };
        true
    }

    /// Start a new cluster at the cursor, wrapping first if it will not fit.
    fn place_new_cluster(&mut self, c: char) {
        // A combining mark with no base to attach to carries no width. xterm discards it.
        let Some(cluster) = Cluster::from_char(c) else {
            return;
        };
        let width = cluster.width() as usize;

        if self.cursor.col + width > self.cols {
            self.wrap();
        }

        let at = self.cursor;
        self.write_cluster(at, cluster);
        self.cursor.col = at.col + width;
        self.open_cluster = Some(at);
    }

    /// Place `cluster` at `at`, clearing whatever it overlaps.
    fn write_cluster(&mut self, at: Cursor, cluster: Cluster) {
        let width = cluster.width() as usize;
        let attrs = self.attrs;

        self.clear_span(at, width);

        self.lines[at.row][at.col] = Cell {
            content: CellContent::Head(cluster),
            attrs,
        };
        for col in (at.col + 1)..(at.col + width).min(self.cols) {
            self.lines[at.row][col] = Cell {
                content: CellContent::Continuation,
                attrs,
            };
        }
    }

    /// Erase the `width` cells at `at`, widened to cover any cluster it partially overlaps.
    ///
    /// Overwriting half of a wide cluster must erase all of it, or the surviving
    /// continuation cells become orphans that render as stale glyph fragments.
    fn clear_span(&mut self, at: Cursor, width: usize) {
        let line = &mut self.lines[at.row];

        let mut start = at.col;
        while start > 0 && line[start].content == CellContent::Continuation {
            start -= 1;
        }

        let mut end = (at.col + width).min(self.cols);
        while end < self.cols && line[end].content == CellContent::Continuation {
            end += 1;
        }

        let attrs = self.attrs;
        for cell in &mut line[start..end] {
            cell.clear(attrs);
        }
    }

    fn content_at(&self, at: Cursor) -> Option<&CellContent> {
        Some(&self.lines.get(at.row)?.get(at.col)?.content)
    }

    // ---- cursor and control functions --------------------------------------

    fn wrap(&mut self) {
        self.carriage_return();
        self.linefeed();
    }

    pub fn linefeed(&mut self) {
        self.open_cluster = None;
        if self.cursor.row + 1 == self.rows {
            self.scroll_up();
        } else {
            self.cursor.row += 1;
        }
    }

    pub fn carriage_return(&mut self) {
        self.open_cluster = None;
        self.cursor.col = 0;
    }

    pub fn backspace(&mut self) {
        self.open_cluster = None;
        self.cursor.col = self.cursor.col.saturating_sub(1);
    }

    /// Advance to the next 8-column tab stop.
    pub fn tab(&mut self) {
        self.open_cluster = None;
        let next = (self.cursor.col / 8 + 1) * 8;
        self.cursor.col = next.min(self.cols - 1);
    }

    fn scroll_up(&mut self) {
        self.lines.rotate_left(1);
        if let Some(last) = self.lines.last_mut() {
            last.fill(Cell::default());
        }
    }

    /// Move the cursor to a zero-indexed position, clamped to the grid.
    pub fn goto(&mut self, row: usize, col: usize) {
        self.open_cluster = None;
        self.cursor = Cursor {
            row: row.min(self.rows - 1),
            col: col.min(self.cols - 1),
        };
    }

    pub fn move_up(&mut self, n: usize) {
        self.open_cluster = None;
        self.cursor.row = self.cursor.row.saturating_sub(n);
    }

    pub fn move_down(&mut self, n: usize) {
        self.open_cluster = None;
        self.cursor.row = (self.cursor.row + n).min(self.rows - 1);
    }

    pub fn move_left(&mut self, n: usize) {
        self.open_cluster = None;
        self.cursor.col = self.cursor.col.saturating_sub(n);
    }

    pub fn move_right(&mut self, n: usize) {
        self.open_cluster = None;
        self.cursor.col = (self.cursor.col + n).min(self.cols - 1);
    }

    // ---- erasing -----------------------------------------------------------

    pub fn erase_line(&mut self, mode: LineErase) {
        self.open_cluster = None;
        let attrs = self.attrs;
        let (start, end) = match mode {
            LineErase::ToEnd => (self.cursor.col, self.cols),
            LineErase::ToStart => (0, self.cursor.col + 1),
            LineErase::All => (0, self.cols),
        };
        for cell in &mut self.lines[self.cursor.row][start..end.min(self.cols)] {
            cell.clear(attrs);
        }
    }

    pub fn erase_screen(&mut self, mode: ScreenErase) {
        self.open_cluster = None;
        let attrs = self.attrs;
        let rows = match mode {
            ScreenErase::Below => {
                self.erase_line(LineErase::ToEnd);
                (self.cursor.row + 1)..self.rows
            }
            ScreenErase::Above => {
                self.erase_line(LineErase::ToStart);
                0..self.cursor.row
            }
            ScreenErase::All => 0..self.rows,
        };
        for row in rows {
            for cell in &mut self.lines[row] {
                cell.clear(attrs);
            }
        }
    }
}
