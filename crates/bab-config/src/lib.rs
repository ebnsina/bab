//! User configuration.
//!
//! Every field has a default, and every section is optional, so an empty file and a
//! missing file behave identically. A malformed file is reported and then ignored:
//! a terminal that refuses to open because of a typo in a colour is a terminal you
//! cannot use to fix the typo.

pub mod color;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use color::{Color, ColorError};

/// The whole configuration.
#[derive(Clone, PartialEq, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Config {
    pub font: Font,
    pub window: Window,
    pub cursor: Cursor,
    pub colors: Colors,
}

/// Fonts, given as file paths.
///
/// Paths rather than family names, for now. Resolving a name means a system font
/// database (`fontique`), and shipping a chain that resolves differently on every
/// machine is exactly what `bab` bundles fonts to avoid.
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Font {
    /// Size in points. Physical pixels are this times the display scale.
    pub size: f32,
    /// The primary face. `None` uses the bundled JetBrains Mono Nerd Font.
    pub file: Option<PathBuf>,
    /// Faces tried in order when the primary lacks a glyph. `None` uses the bundled
    /// Noto Sans Bengali, without which Bengali renders as tofu.
    pub fallback: Option<Vec<PathBuf>>,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            size: 14.0,
            file: None,
            fallback: None,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Window {
    /// Inset from the window edge, in points.
    pub padding: f32,
    /// How opaque the background is, from 0 to 1.
    pub opacity: f32,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            padding: 12.0,
            opacity: 0.92,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Cursor {
    pub shape: CursorShape,
    pub blink: bool,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            shape: CursorShape::Block,
            blink: true,
        }
    }
}

/// The palette. Sixteen ANSI slots, the defaults, and two accents.
#[derive(Clone, PartialEq, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Colors {
    pub foreground: Color,
    pub background: Color,
    /// Drawn under the cursor.
    pub accent: Color,
    pub selection: Color,
    /// How opaque the selection tint is. Text under it must stay legible.
    pub selection_alpha: f32,
    /// Exactly sixteen: eight normal, then eight bright.
    pub ansi: Vec<Color>,
}

impl Default for Colors {
    fn default() -> Self {
        Self {
            foreground: Color::new(0xC8, 0xD0, 0xDA),
            background: Color::new(0x11, 0x14, 0x1A),
            accent: Color::new(0x7A, 0xA2, 0xF7),
            selection: Color::new(0x7A, 0xA2, 0xF7),
            selection_alpha: 0.30,
            ansi: vec![
                Color::new(0x1B, 0x1F, 0x27),
                Color::new(0xF7, 0x76, 0x8E),
                Color::new(0x9E, 0xCE, 0x6A),
                Color::new(0xE0, 0xAF, 0x68),
                Color::new(0x7A, 0xA2, 0xF7),
                Color::new(0xBB, 0x9A, 0xF7),
                Color::new(0x7D, 0xCF, 0xFF),
                Color::new(0xA9, 0xB1, 0xD6),
                Color::new(0x41, 0x48, 0x68),
                Color::new(0xFF, 0x93, 0xA8),
                Color::new(0xB9, 0xF2, 0x7C),
                Color::new(0xFF, 0xC7, 0x77),
                Color::new(0x9A, 0xBD, 0xF5),
                Color::new(0xD3, 0xB4, 0xFF),
                Color::new(0xA4, 0xDA, 0xFF),
                Color::new(0xE6, 0xEB, 0xF4),
            ],
        }
    }
}

impl Config {
    /// Where the config file lives: `$BAB_CONFIG`, else `$XDG_CONFIG_HOME/bab/bab.toml`,
    /// else `~/.config/bab/bab.toml`.
    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        if let Some(explicit) = std::env::var_os("BAB_CONFIG") {
            return Some(PathBuf::from(explicit));
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("bab").join("bab.toml"))
    }

    /// Parse a config from TOML.
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        let config: Self = toml::from_str(text)?;
        Ok(config.sanitized())
    }

    /// Load from `path`, or return defaults when it does not exist.
    ///
    /// A malformed file yields defaults and an explanation. Refusing to start would
    /// leave the user without a terminal in which to fix it.
    pub fn load(path: &Path) -> (Self, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None);
            }
            Err(error) => {
                return (
                    Self::default(),
                    Some(format!("{}: {error}", path.display())),
                );
            }
        };

        match Self::parse(&text) {
            Ok(config) => (config, None),
            Err(error) => (
                Self::default(),
                Some(format!("{}: {error}", path.display())),
            ),
        }
    }

    /// Clamp values that would otherwise produce an unusable window.
    ///
    /// A zero font size divides by zero downstream, and a palette of the wrong length
    /// would panic on index. Correcting quietly beats failing to start.
    #[must_use]
    fn sanitized(mut self) -> Self {
        self.font.size = self.font.size.clamp(4.0, 288.0);
        self.window.padding = self.window.padding.clamp(0.0, 200.0);
        self.window.opacity = self.window.opacity.clamp(0.1, 1.0);
        self.colors.selection_alpha = self.colors.selection_alpha.clamp(0.0, 1.0);

        let defaults = Colors::default().ansi;
        if self.colors.ansi.len() != 16 {
            self.colors.ansi = defaults;
        }
        self
    }
}
