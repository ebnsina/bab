//! How many cells a character occupies.
//!
//! This must agree with the `wcwidth()` the application on the other end of the pty
//! called, or the two disagree about where the cursor is and the screen corrupts.
//! It is the whole of `docs/adr/0001-width-contract.md` in one function.

use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};
use unicode_width::UnicodeWidthChar;

/// Cells occupied by `c`, matching what applications compute.
///
/// Applications sum `wcwidth()` over **codepoints**. Combining marks — both the
/// non-spacing kind (`Mn`, like hasant) and the *spacing* kind (`Mc`, like the Bengali
/// vowel signs) — occupy no cell of their own. Format characters such as `ZWJ` do not
/// either.
///
/// The `unicode-width` crate disagrees about spacing marks: it gives `ী` (U+09C0) a
/// width of one, while the system `wcwidth` gives zero. Trusting the crate made `bab`
/// allocate nine cells for a word that zsh had laid out in eight, and the line ate
/// itself one cell at a time.
/// Categories that occupy no cell.
///
/// Only these. The wider `Other` *group* also contains `Co`, Private Use — which is
/// where every Nerd Font icon lives. Zeroing that group made the grid discard the
/// icons in a shell prompt outright, since a zero-width character with no base to
/// attach to is dropped.
const fn is_zero_width(category: GeneralCategory) -> bool {
    matches!(
        category,
        // Combining marks: hasant, the Bengali vowel signs, accents.
        GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark
            // Format characters: ZWJ, ZWNJ, variation selectors.
            | GeneralCategory::Format
            // Control characters never advance the cursor by printing.
            | GeneralCategory::Control
    )
}

#[must_use]
pub fn char_cells(c: char) -> usize {
    if is_zero_width(c.general_category()) {
        return 0;
    }
    c.width().unwrap_or(0)
}

/// Cells occupied by a grapheme cluster: the sum over its codepoints.
///
/// Never a string-level width. An application never sees the cluster — it sees the
/// codepoints, and adds up their widths one at a time.
#[must_use]
pub fn cluster_cells(cluster: &str) -> usize {
    cluster.chars().map(char_cells).sum()
}
