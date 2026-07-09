//! Keys, modifiers, and the bytes they send.

/// Modifier keys held during an event.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const NONE: Self = Self(0);
    pub const SHIFT: Self = Self(1 << 0);
    pub const ALT: Self = Self(1 << 1);
    pub const CONTROL: Self = Self(1 << 2);
    pub const SUPER: Self = Self(1 << 3);

    /// Build from a raw bitmask, ignoring bits we do not define.
    #[must_use]
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits & 0b1111)
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The xterm modifier parameter: a bitfield offset by one.
    ///
    /// `CSI 1 ; 5 A` is ctrl-up, because 5 = 1 + 4. Sequences omit it when no
    /// modifier is held, which is why callers must check [`Self::is_empty`] first.
    #[must_use]
    pub const fn xterm_param(self) -> u8 {
        let mut param = 1;
        if self.contains(Self::SHIFT) {
            param += 1;
        }
        if self.contains(Self::ALT) {
            param += 2;
        }
        if self.contains(Self::CONTROL) {
            param += 4;
        }
        if self.contains(Self::SUPER) {
            param += 8;
        }
        param
    }
}

impl std::ops::BitOr for Modifiers {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        self.union(other)
    }
}

/// A key press, already resolved from the platform's key event.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Key {
    /// A character the user typed, after the platform applied shift and dead keys.
    Char(char),

    Enter,
    Tab,
    Backspace,
    Escape,

    Up,
    Down,
    Right,
    Left,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,

    /// `F1` through `F12`.
    Function(u8),
}
