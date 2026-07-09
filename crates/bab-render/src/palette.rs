//! Resolving terminal colors to RGBA.
//!
//! A placeholder until `bab-theme` lands, but a deliberate one: the default scheme is
//! the first thing anyone sees, and a terminal that ships with xterm's 1987 primaries
//! looks unfinished no matter how good the renderer is.

use bab_vt::{Attrs, Color, Flags};

/// The 16 ANSI colors, the defaults, and how opaque the window is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Palette {
    pub foreground: [f32; 4],
    pub background: [f32; 4],
    /// Drawn under the cursor.
    pub accent: [f32; 4],
    /// Drawn behind selected cells. Translucent, so the text stays legible.
    pub selection: [f32; 4],
    pub ansi: [[f32; 4]; 16],
}

const fn rgb(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

impl Default for Palette {
    /// A cool, low-glare dark scheme: desaturated primaries so long output does not
    /// vibrate, and a background just off black so the window reads as a surface
    /// rather than a hole.
    fn default() -> Self {
        Self {
            foreground: rgb(0xC8, 0xD0, 0xDA),
            background: rgb(0x11, 0x14, 0x1A),
            accent: rgb(0x7A, 0xA2, 0xF7),
            selection: [0.48, 0.63, 0.97, 0.30],
            ansi: [
                rgb(0x1B, 0x1F, 0x27), // black
                rgb(0xF7, 0x76, 0x8E), // red
                rgb(0x9E, 0xCE, 0x6A), // green
                rgb(0xE0, 0xAF, 0x68), // yellow
                rgb(0x7A, 0xA2, 0xF7), // blue
                rgb(0xBB, 0x9A, 0xF7), // magenta
                rgb(0x7D, 0xCF, 0xFF), // cyan
                rgb(0xA9, 0xB1, 0xD6), // white
                rgb(0x41, 0x48, 0x68), // bright black
                rgb(0xFF, 0x93, 0xA8), // bright red
                rgb(0xB9, 0xF2, 0x7C), // bright green
                rgb(0xFF, 0xC7, 0x77), // bright yellow
                rgb(0x9A, 0xBD, 0xF5), // bright blue
                rgb(0xD3, 0xB4, 0xFF), // bright magenta
                rgb(0xA4, 0xDA, 0xFF), // bright cyan
                rgb(0xE6, 0xEB, 0xF4), // bright white
            ],
        }
    }
}

impl Palette {
    /// How opaque the window background is, from 0 to 1.
    #[must_use]
    pub const fn background_alpha(&self) -> f32 {
        self.background[3]
    }

    /// Set the window's opacity. Below 1 the shell behind shows through.
    pub const fn set_background_alpha(&mut self, alpha: f32) {
        self.background[3] = alpha;
    }

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
            // A reversed cell is a solid block, whatever the window's opacity.
            bg[3] = 1.0;
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
