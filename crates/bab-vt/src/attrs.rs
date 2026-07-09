//! Character attributes: colors and rendition flags.

/// A cell color.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Color {
    /// The theme's default foreground or background, depending on the slot.
    #[default]
    Default,
    /// An index into the 256-color palette.
    Indexed(u8),
    /// A direct 24-bit color.
    Rgb(u8, u8, u8),
}

/// Rendition flags, packed into a byte.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Flags(u8);

impl Flags {
    pub const NONE: Self = Self(0);
    pub const BOLD: Self = Self(1 << 0);
    pub const DIM: Self = Self(1 << 1);
    pub const ITALIC: Self = Self(1 << 2);
    pub const UNDERLINE: Self = Self(1 << 3);
    pub const REVERSE: Self = Self(1 << 4);
    pub const HIDDEN: Self = Self(1 << 5);
    pub const STRIKETHROUGH: Self = Self(1 << 6);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn insert(&mut self, other: Self) {
        self.0 |= other.0;
    }

    pub const fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}

/// The full rendition state applied to a cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Attrs {
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

impl Attrs {
    /// Reset to defaults, as `SGR 0` does.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}
