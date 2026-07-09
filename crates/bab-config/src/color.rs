//! Colors, written the way people write them.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// An 8-bit RGB color, parsed from `#rrggbb` or `#rgb`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Linear-ish RGBA with the given alpha, which is what a renderer wants.
    #[must_use]
    pub fn to_rgba(self, alpha: f32) -> [f32; 4] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
            alpha,
        ]
    }

    /// Parse `#rrggbb`, `#rgb`, or the same without the leading hash.
    ///
    /// Themes in the wild use every spelling. Rejecting one because of a missing `#`
    /// would be pedantry a user has to debug.
    pub fn parse(text: &str) -> Result<Self, ColorError> {
        let hex = text.trim().trim_start_matches('#');

        let expand = |c: u8| c * 17;
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()
            .ok_or_else(|| ColorError(text.to_owned()))?;

        match digits[..] {
            [r, g, b] => Ok(Self::new(expand(r), expand(g), expand(b))),
            [r1, r0, g1, g0, b1, b0] => Ok(Self::new(r1 * 16 + r0, g1 * 16 + g0, b1 * 16 + b0)),
            _ => Err(ColorError(text.to_owned())),
        }
    }
}

/// A color that could not be parsed, with the text the user wrote.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ColorError(pub String);

impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a colour like \"#7aa2f7\"", self.0)
    }
}

impl std::error::Error for ColorError {}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(de::Error::custom)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
