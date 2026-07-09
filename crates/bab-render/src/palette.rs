//! Resolving terminal colors to linear RGBA.
//!
//! A placeholder until `bab-theme` lands. The 16 ANSI slots and the default
//! foreground and background are all a renderer needs.

use bab_vt::{Attrs, Color, Flags};

/// The 16 ANSI colors plus defaults.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub foreground: [f32; 4],
    pub background: [f32; 4],
    pub ansi: [[f32; 4]; 16],
}

const fn rgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            foreground: rgb(0xD0, 0xD0, 0xD0),
            background: rgb(0x10, 0x10, 0x12),
            ansi: [
                rgb(0x1C, 0x1C, 0x1C),
                rgb(0xE0, 0x50, 0x50),
                rgb(0x50, 0xC0, 0x70),
                rgb(0xD0, 0xB0, 0x40),
                rgb(0x50, 0x90, 0xE0),
                rgb(0xB0, 0x60, 0xD0),
                rgb(0x40, 0xB0, 0xC0),
                rgb(0xC0, 0xC0, 0xC0),
                rgb(0x50, 0x50, 0x50),
                rgb(0xFF, 0x70, 0x70),
                rgb(0x70, 0xE0, 0x90),
                rgb(0xF0, 0xD0, 0x60),
                rgb(0x70, 0xB0, 0xFF),
                rgb(0xD0, 0x80, 0xF0),
                rgb(0x60, 0xD0, 0xE0),
                rgb(0xFF, 0xFF, 0xFF),
            ],
        }
    }
}

impl Palette {
    /// Resolve a color in the foreground slot.
    #[must_use]
    pub fn resolve_fg(&self, color: Color) -> [f32; 4] {
        self.resolve(color, self.foreground)
    }

    /// Resolve a color in the background slot.
    #[must_use]
    pub fn resolve_bg(&self, color: Color) -> [f32; 4] {
        self.resolve(color, self.background)
    }

    fn resolve(&self, color: Color, default: [f32; 4]) -> [f32; 4] {
        match color {
            Color::Default => default,
            Color::Indexed(index) => self.indexed(index),
            Color::Rgb(r, g, b) => rgb(r, g, b),
        }
    }

    /// The xterm 256-color cube, of which the first 16 are the ANSI slots.
    fn indexed(&self, index: u8) -> [f32; 4] {
        match index {
            0..=15 => self.ansi[index as usize],
            16..=231 => {
                const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
                let i = index - 16;
                rgb(
                    LEVELS[(i / 36) as usize],
                    LEVELS[((i % 36) / 6) as usize],
                    LEVELS[(i % 6) as usize],
                )
            }
            232..=255 => {
                let level = 8 + (index - 232) * 10;
                rgb(level, level, level)
            }
        }
    }

    /// The foreground and background a cell actually draws with.
    ///
    /// `reverse` swaps them, and it must be applied after resolving defaults — a
    /// reversed cell paints the default foreground as its background.
    #[must_use]
    pub fn colors_for(&self, attrs: Attrs) -> ([f32; 4], [f32; 4]) {
        let mut fg = self.resolve_fg(attrs.fg);
        let mut bg = self.resolve_bg(attrs.bg);

        if attrs.flags.contains(Flags::REVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if attrs.flags.contains(Flags::DIM) {
            for channel in &mut fg[..3] {
                *channel *= 0.6;
            }
        }
        if attrs.flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        (fg, bg)
    }
}
