//! A loaded font face, and the fallback chain that resolves a cluster to one.

use std::sync::Arc;

use anyhow::{Context, Result};
use harfrust::{FontRef, ShaperData};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};

/// Characters a font is never expected to have a glyph for, and whose absence
/// must not disqualify it during fallback.
///
/// A font shapes `ZWJ` and `ZWNJ` by their effect on joining, not by drawing them.
/// Requiring coverage would push every conjunct to a fallback face.
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200C}' | '\u{200D}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

/// A font face, ready to shape.
///
/// Holds its own bytes so the shaper can be rebuilt per call: `harfrust`'s `Shaper`
/// borrows a `FontRef`, which borrows the bytes, and a struct cannot borrow itself.
/// [`ShaperData`] carries the expensive precomputation and is built once.
pub struct Face {
    name: String,
    bytes: Arc<Vec<u8>>,
    shaper_data: ShaperData,
    units_per_em: u16,
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Face")
            .field("name", &self.name)
            .field("units_per_em", &self.units_per_em)
            .finish_non_exhaustive()
    }
}

impl Face {
    /// Parse a font from its bytes.
    pub fn new(name: impl Into<String>, bytes: impl Into<Arc<Vec<u8>>>) -> Result<Self> {
        let name = name.into();
        let bytes = bytes.into();

        let font = FontRef::new(&bytes).with_context(|| format!("failed to parse font {name}"))?;
        let shaper_data = ShaperData::new(&font);
        let units_per_em = font
            .metrics(Size::unscaled(), LocationRef::default())
            .units_per_em;

        Ok(Self {
            name,
            bytes,
            shaper_data,
            units_per_em,
        })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Font design units per em. Shaped positions are in these units.
    #[must_use]
    pub const fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Whether this face has a glyph for `c`.
    #[must_use]
    pub fn has_glyph(&self, c: char) -> bool {
        let Ok(font) = FontRef::new(&self.bytes) else {
            return false;
        };
        font.charmap().map(c).is_some()
    }

    /// Whether this face can draw every visible character of `cluster`.
    #[must_use]
    pub fn covers(&self, cluster: &str) -> bool {
        let Ok(font) = FontRef::new(&self.bytes) else {
            return false;
        };
        let charmap = font.charmap();
        cluster
            .chars()
            .filter(|c| !is_invisible(*c))
            .all(|c| charmap.map(c).is_some())
    }

    pub(crate) fn font(&self) -> Result<FontRef<'_>> {
        FontRef::new(&self.bytes).context("failed to reparse font")
    }

    pub(crate) const fn shaper_data(&self) -> &ShaperData {
        &self.shaper_data
    }
}

/// An ordered fallback chain. The first face that covers a cluster shapes it.
///
/// The chain is explicit and pinned rather than delegated to system fallback, which
/// resolves differently on every machine and makes bug reports unreproducible.
#[derive(Debug)]
pub struct FontStack {
    faces: Vec<Face>,
}

impl FontStack {
    /// Build a stack from `faces`, primary first.
    ///
    /// Returns an error when empty: there would be nothing to render tofu with.
    pub fn new(faces: Vec<Face>) -> Result<Self> {
        anyhow::ensure!(!faces.is_empty(), "a font stack needs at least one face");
        Ok(Self { faces })
    }

    #[must_use]
    pub fn primary(&self) -> &Face {
        &self.faces[0]
    }

    #[must_use]
    pub fn faces(&self) -> &[Face] {
        &self.faces
    }

    /// The first face covering `cluster`, with its index.
    ///
    /// Falls back to the primary face when nothing covers it, so the caller renders
    /// tofu rather than nothing. A missing glyph is a visible bug; a missing cell is
    /// a silent one.
    #[must_use]
    pub fn resolve(&self, cluster: &str) -> (usize, &Face) {
        self.faces
            .iter()
            .enumerate()
            .find(|(_, face)| face.covers(cluster))
            .unwrap_or((0, &self.faces[0]))
    }
}
