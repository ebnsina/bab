//! The cell model.
//!
//! A cell holds a whole grapheme cluster, not a codepoint. `ব` + `্` + `ল` is three
//! codepoints, one cluster, and one conjunct glyph — so it must live in one cell.
//!
//! Cell *width* is decided by [`unicode_width`], never by the shaper. See
//! `docs/adr/0001-width-contract.md`: the grid allocates exactly the cells a TUI app
//! computed with `wcwidth()`, and shaping is confined to rendering within that span.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::attrs::Attrs;

/// A grapheme cluster occupying one or more cells.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cluster {
    text: String,
    width: u16,
}

impl Cluster {
    /// Build a cluster from a single character, or `None` if it carries no width
    /// of its own (an orphan combining mark with no base to attach to).
    #[must_use]
    pub fn from_char(c: char) -> Option<Self> {
        let width = u16::try_from(c.width()?).ok()?;
        (width > 0).then(|| Self {
            text: c.to_string(),
            width,
        })
    }

    /// The cluster's codepoints, in logical order. Copy and paste use this.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Cells occupied. Always at least 1.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Append `c` if the result is still a single grapheme cluster.
    ///
    /// Returns the resulting width on success, leaving `self` unchanged on failure.
    pub fn try_extend(&mut self, c: char) -> Option<u16> {
        let mut candidate = String::with_capacity(self.text.len() + c.len_utf8());
        candidate.push_str(&self.text);
        candidate.push(c);

        if !is_single_cluster(&candidate) {
            return None;
        }

        let width = u16::try_from(candidate.width()).unwrap_or(u16::MAX).max(1);
        self.text = candidate;
        self.width = width;
        Some(width)
    }
}

/// Whether `s` forms exactly one extended grapheme cluster.
///
/// Relies on UAX #29 GB9c (Unicode 15.1), which keeps Indic conjuncts together.
#[must_use]
pub fn is_single_cluster(s: &str) -> bool {
    let mut graphemes = s.graphemes(true);
    graphemes.next().is_some() && graphemes.next().is_none()
}

/// What a cell holds.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum CellContent {
    /// Nothing has been printed here.
    #[default]
    Empty,
    /// The start of a cluster, which spans [`Cluster::width`] cells from here.
    Head(Cluster),
    /// Covered by a cluster whose head lies to the left.
    Continuation,
}

/// One cell of the grid.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Cell {
    pub content: CellContent,
    pub attrs: Attrs,
}

impl Cell {
    /// The cluster starting here, if any.
    #[must_use]
    pub const fn cluster(&self) -> Option<&Cluster> {
        match &self.content {
            CellContent::Head(cluster) => Some(cluster),
            _ => None,
        }
    }

    /// Reset to an empty cell carrying `attrs`.
    pub fn clear(&mut self, attrs: Attrs) {
        self.content = CellContent::Empty;
        self.attrs = attrs;
    }
}
