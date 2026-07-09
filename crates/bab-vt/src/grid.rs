//! The character grid: cursor, lines, scrollback, and cluster placement.

use std::collections::VecDeque;

use crate::attrs::Attrs;
use crate::cell::{Cell, CellContent, Cluster};

/// Cursor position, in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

/// Cursor position and rendition, saved by `DECSC` and `CSI s`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SavedCursor {
    pub cursor: Cursor,
    pub attrs: Attrs,
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

/// The inclusive row range affected by scrolling, set by `DECSTBM`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ScrollRegion {
    top: usize,
    bottom: usize,
}

/// A grid of cells with a cursor, a scroll region, and optional scrollback.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grid {
    lines: Vec<Vec<Cell>>,
    scrollback: VecDeque<Vec<Cell>>,
    scrollback_limit: usize,
    cols: usize,
    rows: usize,
    cursor: Cursor,
    saved_cursor: SavedCursor,
    attrs: Attrs,
    region: ScrollRegion,
    /// `DECAWM`. When false, printing at the right edge overwrites the last cell.
    pub autowrap: bool,
    /// How many lines of history the viewport is scrolled up by. Zero is the live
    /// screen. Rendering and selection read through this; the terminal never does.
    viewport_offset: usize,
    /// Head of the cluster most recently printed, if the next character could still
    /// extend it. Any cursor movement or erase invalidates this.
    open_cluster: Option<Cursor>,
}

impl Grid {
    #[must_use]
    pub fn new(rows: usize, cols: usize, scrollback_limit: usize) -> Self {
        let rows = rows.max(1);
        let cols = cols.max(1);
        Self {
            lines: vec![vec![Cell::default(); cols]; rows],
            scrollback: VecDeque::new(),
            scrollback_limit,
            cols,
            rows,
            cursor: Cursor::default(),
            saved_cursor: SavedCursor::default(),
            attrs: Attrs::default(),
            region: ScrollRegion {
                top: 0,
                bottom: rows - 1,
            },
            autowrap: true,
            viewport_offset: 0,
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

    /// Lines scrolled off the top, oldest first.
    #[must_use]
    pub fn scrollback(&self) -> &VecDeque<Vec<Cell>> {
        &self.scrollback
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

    // ---- viewport ----------------------------------------------------------

    /// Lines the viewport is scrolled up by. Zero means the live screen.
    #[must_use]
    pub const fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    /// The furthest the viewport can scroll back.
    #[must_use]
    pub fn max_scroll(&self) -> usize {
        self.scrollback.len()
    }

    /// Scroll the viewport back into history, saturating at the oldest line.
    pub fn scroll_back(&mut self, lines: usize) {
        self.viewport_offset = (self.viewport_offset + lines).min(self.max_scroll());
    }

    /// Scroll the viewport toward the live screen.
    pub fn scroll_forward(&mut self, lines: usize) {
        self.viewport_offset = self.viewport_offset.saturating_sub(lines);
    }

    pub const fn scroll_to_bottom(&mut self) {
        self.viewport_offset = 0;
    }

    /// The cell at a viewport position, which may lie in scrollback.
    ///
    /// Everything that draws or selects reads through here. Terminal state — printing,
    /// the cursor, erasing — always addresses the live screen and never this.
    #[must_use]
    pub fn display_cell(&self, row: usize, col: usize) -> Option<&Cell> {
        if self.viewport_offset == 0 {
            return self.cell(row, col);
        }
        // The viewport shows the last `offset` scrollback lines, then the screen.
        let history = self.scrollback.len();
        let first = history - self.viewport_offset;

        if first + row < history {
            self.scrollback.get(first + row)?.get(col)
        } else {
            self.lines.get(first + row - history)?.get(col)
        }
    }

    /// The text of a viewport row, in logical order.
    #[must_use]
    pub fn display_row_text(&self, row: usize) -> String {
        (0..self.cols)
            .filter_map(|col| self.display_cell(row, col))
            .filter_map(Cell::cluster)
            .map(Cluster::text)
            .collect()
    }

    /// Whether the viewport is showing the live screen, where the cursor lives.
    #[must_use]
    pub const fn is_at_bottom(&self) -> bool {
        self.viewport_offset == 0
    }

    /// The text of a scrollback line, oldest first.
    #[must_use]
    pub fn scrollback_text(&self, index: usize) -> Option<String> {
        let line = self.scrollback.get(index)?;
        Some(
            line.iter()
                .filter_map(Cell::cluster)
                .map(Cluster::text)
                .collect(),
        )
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
            if self.autowrap {
                self.wrap();
            } else {
                // With autowrap off the cursor pins to the last usable column.
                self.cursor.col = self.cols.saturating_sub(width);
            }
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
        let attrs = self.attrs;
        let line = &mut self.lines[at.row];

        let mut start = at.col;
        while start > 0 && line[start].content == CellContent::Continuation {
            start -= 1;
        }

        let mut end = (at.col + width).min(self.cols);
        while end < self.cols && line[end].content == CellContent::Continuation {
            end += 1;
        }

        for cell in &mut line[start..end] {
            cell.clear(attrs);
        }
    }

    /// Clear clusters that lost their span, after cells were shifted or truncated.
    ///
    /// Shifting cells can leave a head whose continuations were displaced, or a
    /// continuation whose head is gone. Both must be erased, not rendered.
    fn repair_line(&mut self, row: usize) {
        let cols = self.cols;
        let line = &mut self.lines[row];
        let mut col = 0;

        while col < cols {
            let width = match &line[col].content {
                CellContent::Head(cluster) => cluster.width() as usize,
                // Reaching a continuation here means no head claimed it.
                CellContent::Continuation => {
                    line[col].content = CellContent::Empty;
                    col += 1;
                    continue;
                }
                CellContent::Empty => {
                    col += 1;
                    continue;
                }
            };

            let intact = col + width <= cols
                && (col + 1..col + width).all(|i| line[i].content == CellContent::Continuation);

            if intact {
                col += width;
            } else {
                line[col].content = CellContent::Empty;
                col += 1;
            }
        }
    }

    fn content_at(&self, at: Cursor) -> Option<&CellContent> {
        Some(&self.lines.get(at.row)?.get(at.col)?.content)
    }

    fn blank_line(&self) -> Vec<Cell> {
        vec![
            Cell {
                content: CellContent::Empty,
                attrs: self.attrs
            };
            self.cols
        ]
    }

    // ---- cursor and control functions --------------------------------------

    fn wrap(&mut self) {
        self.carriage_return();
        self.linefeed();
    }

    pub fn linefeed(&mut self) {
        self.open_cluster = None;
        if self.cursor.row == self.region.bottom {
            self.scroll_up(1);
        } else if self.cursor.row + 1 < self.rows {
            self.cursor.row += 1;
        }
    }

    /// Reverse index: move up, scrolling the region down at the top margin.
    pub fn reverse_linefeed(&mut self) {
        self.open_cluster = None;
        if self.cursor.row == self.region.top {
            self.scroll_down(1);
        } else {
            self.cursor.row = self.cursor.row.saturating_sub(1);
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

    pub fn save_cursor(&mut self) {
        self.saved_cursor = SavedCursor {
            cursor: self.cursor,
            attrs: self.attrs,
        };
    }

    pub fn restore_cursor(&mut self) {
        self.open_cluster = None;
        let saved = self.saved_cursor;
        self.attrs = saved.attrs;
        self.cursor = Cursor {
            row: saved.cursor.row.min(self.rows - 1),
            col: saved.cursor.col.min(self.cols - 1),
        };
    }

    /// Set the scroll region from inclusive, zero-indexed rows. Homes the cursor.
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        if top >= bottom || bottom >= self.rows {
            self.region = ScrollRegion {
                top: 0,
                bottom: self.rows - 1,
            };
        } else {
            self.region = ScrollRegion { top, bottom };
        }
        self.goto(0, 0);
    }

    /// Scroll the region up by `n`, moving evicted lines into scrollback.
    pub fn scroll_up(&mut self, n: usize) {
        self.open_cluster = None;
        let n = n.min(self.region.bottom - self.region.top + 1);

        for _ in 0..n {
            let evicted = self.lines.remove(self.region.top);
            // Only content leaving the top of the screen enters scrollback.
            if self.region.top == 0 && self.scrollback_limit > 0 {
                let full = self.scrollback.len() == self.scrollback_limit;
                if full {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(evicted);

                // A reader scrolled into history stays on the lines they are reading
                // rather than being dragged along by new output. Once the oldest line
                // falls off the end there is nothing left to hold on to.
                if self.viewport_offset > 0 && !full {
                    self.viewport_offset = (self.viewport_offset + 1).min(self.scrollback.len());
                }
            }
            self.lines.insert(self.region.bottom, self.blank_line());
        }
    }

    /// Scroll the region down by `n`. Scrollback is not consulted.
    pub fn scroll_down(&mut self, n: usize) {
        self.open_cluster = None;
        let n = n.min(self.region.bottom - self.region.top + 1);

        for _ in 0..n {
            self.lines.remove(self.region.bottom);
            self.lines.insert(self.region.top, self.blank_line());
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

    // ---- editing -----------------------------------------------------------

    /// `IL`. Insert `n` blank lines at the cursor, within the scroll region.
    pub fn insert_lines(&mut self, n: usize) {
        self.open_cluster = None;
        if !self.cursor_in_region() {
            return;
        }
        let n = n.min(self.region.bottom - self.cursor.row + 1);
        for _ in 0..n {
            self.lines.remove(self.region.bottom);
            self.lines.insert(self.cursor.row, self.blank_line());
        }
    }

    /// `DL`. Delete `n` lines at the cursor, within the scroll region.
    pub fn delete_lines(&mut self, n: usize) {
        self.open_cluster = None;
        if !self.cursor_in_region() {
            return;
        }
        let n = n.min(self.region.bottom - self.cursor.row + 1);
        for _ in 0..n {
            self.lines.remove(self.cursor.row);
            self.lines.insert(self.region.bottom, self.blank_line());
        }
    }

    /// `ICH`. Shift the rest of the line right by `n`, inserting blanks.
    pub fn insert_chars(&mut self, n: usize) {
        self.open_cluster = None;
        let (row, col) = (self.cursor.row, self.cursor.col);
        let n = n.min(self.cols - col);
        let attrs = self.attrs;

        let line = &mut self.lines[row];
        line[col..].rotate_right(n);
        for cell in &mut line[col..col + n] {
            cell.clear(attrs);
        }
        self.repair_line(row);
    }

    /// `DCH`. Shift the rest of the line left by `n`, filling with blanks.
    pub fn delete_chars(&mut self, n: usize) {
        self.open_cluster = None;
        let (row, col) = (self.cursor.row, self.cursor.col);
        let n = n.min(self.cols - col);
        let attrs = self.attrs;

        let line = &mut self.lines[row];
        line[col..].rotate_left(n);
        for cell in &mut line[self.cols - n..] {
            cell.clear(attrs);
        }
        self.repair_line(row);
    }

    /// `ECH`. Blank `n` cells at the cursor without shifting.
    pub fn erase_chars(&mut self, n: usize) {
        self.open_cluster = None;
        let (row, col) = (self.cursor.row, self.cursor.col);
        let end = (col + n).min(self.cols);
        let attrs = self.attrs;

        for cell in &mut self.lines[row][col..end] {
            cell.clear(attrs);
        }
        self.repair_line(row);
    }

    const fn cursor_in_region(&self) -> bool {
        self.cursor.row >= self.region.top && self.cursor.row <= self.region.bottom
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
        self.repair_line(self.cursor.row);
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

    // ---- resize ------------------------------------------------------------

    /// Resize the grid, preserving content anchored at the top-left.
    ///
    /// Lines are not reflowed: a narrower grid truncates rather than rewrapping.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        let rows = rows.max(1);
        let cols = cols.max(1);
        if (rows, cols) == (self.rows, self.cols) {
            return;
        }
        self.open_cluster = None;

        if cols != self.cols {
            self.cols = cols;
            for line in &mut self.lines {
                line.resize(cols, Cell::default());
            }
            for row in 0..self.lines.len() {
                self.repair_line(row);
            }
        }

        while self.lines.len() > rows {
            // Rows lost from the bottom first, so the cursor's context survives.
            if self.cursor.row < self.lines.len() - 1 {
                self.lines.pop();
            } else {
                let evicted = self.lines.remove(0);
                if self.scrollback_limit > 0 {
                    if self.scrollback.len() == self.scrollback_limit {
                        self.scrollback.pop_front();
                    }
                    self.scrollback.push_back(evicted);
                }
                self.cursor.row = self.cursor.row.saturating_sub(1);
            }
        }
        while self.lines.len() < rows {
            self.lines.push(vec![Cell::default(); cols]);
        }

        self.rows = rows;
        self.viewport_offset = self.viewport_offset.min(self.scrollback.len());
        self.region = ScrollRegion {
            top: 0,
            bottom: rows - 1,
        };
        self.cursor.row = self.cursor.row.min(rows - 1);
        self.cursor.col = self.cursor.col.min(cols - 1);
    }
}
