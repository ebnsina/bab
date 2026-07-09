//! `SGR` (Select Graphic Rendition) parsing.

use vte::{Params, ParamsIter};

use crate::attrs::{Attrs, Color, Flags};

/// Apply an `SGR` sequence to `attrs`.
pub fn apply(attrs: &mut Attrs, params: &Params) {
    if params.is_empty() {
        attrs.reset();
        return;
    }

    let mut iter = params.iter();
    while let Some(param) = iter.next() {
        let (&code, subparams) = match param.split_first() {
            Some(split) => split,
            None => continue,
        };
        apply_one(attrs, code, subparams, &mut iter);
    }
}

fn apply_one(attrs: &mut Attrs, code: u16, subparams: &[u16], iter: &mut ParamsIter<'_>) {
    match code {
        0 => attrs.reset(),
        1 => attrs.flags.insert(Flags::BOLD),
        2 => attrs.flags.insert(Flags::DIM),
        3 => attrs.flags.insert(Flags::ITALIC),
        4 => attrs.flags.insert(Flags::UNDERLINE),
        7 => attrs.flags.insert(Flags::REVERSE),
        8 => attrs.flags.insert(Flags::HIDDEN),
        9 => attrs.flags.insert(Flags::STRIKETHROUGH),
        21 | 22 => {
            attrs.flags.remove(Flags::BOLD);
            attrs.flags.remove(Flags::DIM);
        }
        23 => attrs.flags.remove(Flags::ITALIC),
        24 => attrs.flags.remove(Flags::UNDERLINE),
        27 => attrs.flags.remove(Flags::REVERSE),
        28 => attrs.flags.remove(Flags::HIDDEN),
        29 => attrs.flags.remove(Flags::STRIKETHROUGH),

        30..=37 => attrs.fg = Color::Indexed((code - 30) as u8),
        90..=97 => attrs.fg = Color::Indexed((code - 90 + 8) as u8),
        38 => {
            if let Some(color) = extended_color(subparams, iter) {
                attrs.fg = color;
            }
        }
        39 => attrs.fg = Color::Default,

        40..=47 => attrs.bg = Color::Indexed((code - 40) as u8),
        100..=107 => attrs.bg = Color::Indexed((code - 100 + 8) as u8),
        48 => {
            if let Some(color) = extended_color(subparams, iter) {
                attrs.bg = color;
            }
        }
        49 => attrs.bg = Color::Default,

        _ => {}
    }
}

/// Parse the argument of `SGR 38`/`48`.
///
/// Both spellings are accepted: colon-separated subparameters (`38:2::r:g:b`) and the
/// legacy semicolon form (`38;2;r;g;b`), which arrives as separate parameters.
fn extended_color(subparams: &[u16], iter: &mut ParamsIter<'_>) -> Option<Color> {
    if !subparams.is_empty() {
        return from_subparams(subparams);
    }

    match *iter.next()?.first()? {
        2 => {
            let r = next_component(iter)?;
            let g = next_component(iter)?;
            let b = next_component(iter)?;
            Some(Color::Rgb(r, g, b))
        }
        5 => Some(Color::Indexed(next_component(iter)?)),
        _ => None,
    }
}

fn from_subparams(subparams: &[u16]) -> Option<Color> {
    match subparams {
        // `38:2:r:g:b`, and `38:2::r:g:b` where the empty slot is a color space id.
        [2, r, g, b] | [2, _, r, g, b] => {
            Some(Color::Rgb(component(*r), component(*g), component(*b)))
        }
        [5, index] => Some(Color::Indexed(component(*index))),
        _ => None,
    }
}

fn next_component(iter: &mut ParamsIter<'_>) -> Option<u8> {
    iter.next()?.first().copied().map(component)
}

fn component(value: u16) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}
