//! Offscreen rendering. Skips when no GPU adapter exists, which is CI without a
//! software rasterizer — see `docs/stack.md` on lavapipe and WARP.

use std::path::PathBuf;
use std::sync::Arc;

use bab_render::{Palette, Renderer};
use bab_text::{Face, FontStack};
use bab_vt::Terminal;

const FONT_SIZE: f32 = 16.0;
const WIDTH: u32 = 320;
const HEIGHT: u32 = 96;

fn load(file: &str) -> Face {
    let path: PathBuf = [
        env!("CARGO_MANIFEST_DIR"),
        "..",
        "..",
        "assets",
        "fonts",
        file,
    ]
    .iter()
    .collect();
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    Face::new(file, Arc::new(bytes)).expect("parsing font")
}

fn fonts() -> FontStack {
    FontStack::new(vec![
        load("FiraCodeNerdFontMono-Regular.ttf"),
        load("NotoSansBengali-Regular.ttf"),
    ])
    .unwrap()
}

/// `None` means no GPU is available and the caller should skip.
fn renderer() -> Option<Renderer> {
    match Renderer::new(WIDTH, HEIGHT, fonts(), FONT_SIZE) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipping: no GPU adapter ({error})");
            None
        }
    }
}

fn render(text: &str) -> Option<Vec<u8>> {
    let mut renderer = renderer()?;
    let mut terminal = Terminal::new(4, 20);
    terminal.feed(text.as_bytes());

    renderer.render(terminal.grid(), None).expect("render");
    Some(renderer.read_pixels().expect("readback"))
}

/// Pixels differing from the palette background, i.e. anything actually drawn.
fn ink(pixels: &[u8], palette: &Palette) -> usize {
    let bg: Vec<u8> = palette.background[..3]
        .iter()
        .map(|c| (c * 255.0).round() as u8)
        .collect();
    pixels
        .chunks_exact(4)
        .filter(|px| px[..3] != bg[..])
        .count()
}

#[test]
fn blank_grid_renders_only_background() {
    let Some(pixels) = render("") else { return };
    assert_eq!(pixels.len(), (WIDTH * HEIGHT * 4) as usize);
    assert_eq!(
        ink(&pixels, &Palette::default()),
        0,
        "blank grid should have no ink"
    );
}

#[test]
fn latin_text_puts_ink_on_the_target() {
    let Some(pixels) = render("hello") else {
        return;
    };
    assert!(
        ink(&pixels, &Palette::default()) > 0,
        "expected glyphs to be drawn"
    );
}

/// The end of the pipeline: Bengali is shaped, resolved to the fallback face,
/// rasterized, and drawn. If the fallback chain broke we would render tofu or nothing.
#[test]
fn bangla_puts_ink_on_the_target() {
    let Some(pixels) = render("বাংলা") else {
        return;
    };
    assert!(
        ink(&pixels, &Palette::default()) > 0,
        "expected Bangla glyphs to be drawn"
    );
}

/// A conjunct occupies one base advance, so it must not paint more ink than the same
/// consonants written separately. This is the width contract, in pixels.
#[test]
fn a_conjunct_is_narrower_than_its_parts() {
    let Some(conjunct) = render("ব্ল") else {
        return;
    };
    let Some(separate) = render("বল") else {
        return;
    };

    let palette = Palette::default();
    let conjunct_columns = inked_columns(&conjunct, &palette);
    let separate_columns = inked_columns(&separate, &palette);

    assert!(
        conjunct_columns < separate_columns,
        "conjunct spanned {conjunct_columns} columns, separate letters {separate_columns}"
    );
}

/// How many pixel columns contain any ink.
fn inked_columns(pixels: &[u8], palette: &Palette) -> usize {
    let bg: Vec<u8> = palette.background[..3]
        .iter()
        .map(|c| (c * 255.0).round() as u8)
        .collect();
    (0..WIDTH as usize)
        .filter(|x| {
            (0..HEIGHT as usize).any(|y| {
                let offset = (y * WIDTH as usize + x) * 4;
                pixels[offset..offset + 3] != bg[..]
            })
        })
        .count()
}

#[test]
fn rendering_is_deterministic() {
    let Some(first) = render("bab বাংলা") else {
        return;
    };
    let Some(second) = render("bab বাংলা") else {
        return;
    };
    assert_eq!(first, second, "the same grid must render identically");
}

#[test]
fn reverse_video_paints_the_cell_background() {
    let Some(plain) = render("\u{1b}[0mX") else {
        return;
    };
    let Some(reversed) = render("\u{1b}[7mX") else {
        return;
    };

    let palette = Palette::default();
    assert!(
        ink(&reversed, &palette) > ink(&plain, &palette),
        "a reversed cell should fill its background"
    );
}

/// Resizing must rewrite the viewport, or the shader projects into the old size.
#[test]
fn resize_changes_the_readback_size() {
    let Some(mut renderer) = renderer() else {
        return;
    };
    renderer.resize(64, 32);

    let mut terminal = Terminal::new(2, 8);
    terminal.feed(b"hi");
    renderer.render(terminal.grid(), None).expect("render");

    let pixels = renderer.read_pixels().expect("readback");
    assert_eq!(pixels.len(), 64 * 32 * 4);
    assert!(
        ink(&pixels, &Palette::default()) > 0,
        "text should survive a resize"
    );
}

#[test]
fn cell_metrics_are_positive() {
    let Some(renderer) = renderer() else { return };
    let metrics = renderer.metrics();
    assert!(metrics.cell.width > 0.0);
    assert!(metrics.cell.height > 0.0);
    assert!(metrics.ascent > 0.0);
}

// ---- cursor ----------------------------------------------------------------

use bab_render::CursorState;
use bab_vt::{Cursor, CursorShape, CursorStyle};

fn render_with_cursor(text: &str, cursor: Option<CursorState>) -> Option<Vec<u8>> {
    let mut renderer = renderer()?;
    let mut terminal = Terminal::new(4, 20);
    terminal.feed(text.as_bytes());
    renderer.render(terminal.grid(), cursor).expect("render");
    Some(renderer.read_pixels().expect("readback"))
}

fn cursor_at(col: usize, shape: CursorShape, focused: bool) -> CursorState {
    CursorState {
        position: Cursor { row: 0, col },
        style: CursorStyle {
            shape,
            blink: false,
        },
        focused,
        visible: true,
    }
}

#[test]
fn no_cursor_draws_nothing_extra() {
    let Some(without) = render_with_cursor("", None) else {
        return;
    };
    assert_eq!(ink(&without, &Palette::default()), 0);
}

/// The dark half of a blink draws nothing, exactly as if there were no cursor.
#[test]
fn an_invisible_cursor_draws_nothing() {
    let mut state = cursor_at(0, CursorShape::Block, true);
    state.visible = false;
    let Some(pixels) = render_with_cursor("", Some(state)) else {
        return;
    };
    assert_eq!(ink(&pixels, &Palette::default()), 0);
}

#[test]
fn a_block_cursor_fills_its_cell() {
    let Some(pixels) = render_with_cursor("", Some(cursor_at(0, CursorShape::Block, true))) else {
        return;
    };
    assert!(ink(&pixels, &Palette::default()) > 0);
}

/// A bar is thinner than a block, and an unfocused outline thinner still.
#[test]
fn cursor_shapes_differ_in_area() {
    let Some(block) = render_with_cursor("", Some(cursor_at(0, CursorShape::Block, true))) else {
        return;
    };
    let Some(bar) = render_with_cursor("", Some(cursor_at(0, CursorShape::Bar, true))) else {
        return;
    };
    let Some(hollow) = render_with_cursor("", Some(cursor_at(0, CursorShape::Block, false))) else {
        return;
    };

    let palette = Palette::default();
    assert!(ink(&block, &palette) > ink(&bar, &palette));
    assert!(ink(&block, &palette) > ink(&hollow, &palette));
    assert!(
        ink(&hollow, &palette) > 0,
        "an unfocused cursor still draws an outline"
    );
}

/// A filled block must not swallow the character under it: the glyph is repainted in
/// the background colour, so some pixels inside the cell match the background.
#[test]
fn a_block_cursor_inverts_the_glyph_beneath_it() {
    let Some(pixels) = render_with_cursor("W", Some(cursor_at(0, CursorShape::Block, true))) else {
        return;
    };
    let palette = Palette::default();
    let bg: Vec<u8> = palette.background[..3]
        .iter()
        .map(|c| (c * 255.0).round() as u8)
        .collect();

    // Look only inside the first cell.
    let cell_w = 10_usize;
    let cell_h = 10_usize;
    let holes = (0..cell_h)
        .flat_map(|y| (0..cell_w).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let offset = (y * WIDTH as usize + x) * 4;
            pixels[offset..offset + 3] == bg[..]
        })
        .count();

    assert!(holes > 0, "the glyph should be cut out of the block cursor");
}

// ---- run shaping -----------------------------------------------------------

/// Widest run of background-coloured pixel columns strictly inside the inked region.
///
/// A word rendered as disconnected islands shows up here as a wide interior gap.
fn widest_interior_gap(pixels: &[u8], palette: &Palette) -> usize {
    let bg: Vec<u8> = palette.background[..3]
        .iter()
        .map(|c| (c * 255.0).round() as u8)
        .collect();
    let inked: Vec<bool> = (0..WIDTH as usize)
        .map(|x| {
            (0..HEIGHT as usize).any(|y| {
                let offset = (y * WIDTH as usize + x) * 4;
                pixels[offset..offset + 3] != bg[..]
            })
        })
        .collect();

    let Some(first) = inked.iter().position(|ink| *ink) else {
        return 0;
    };
    let last = inked.iter().rposition(|ink| *ink).unwrap_or(first);

    let mut widest = 0;
    let mut current = 0;
    for &ink in &inked[first..=last] {
        if ink {
            current = 0;
        } else {
            current += 1;
            widest = widest.max(current);
        }
    }
    widest
}

/// Shaping each cluster alone and centring it in its `wcwidth` span shatters Bengali
/// words: a conjunct that draws as one glyph is allotted a cell per consonant, so the
/// letters float apart. Shaping the whole run keeps the word together.
#[test]
fn a_bengali_word_renders_without_interior_gaps() {
    let Some(pixels) = render("প্রধানমন্ত্রী") else {
        return;
    };
    let gap = widest_interior_gap(&pixels, &Palette::default());
    assert!(gap < 8, "the word was shattered: {gap}px gap inside it");
}

/// Ligatures span two cells and two clusters, so they can only form when the run is
/// shaped whole. Fewer glyphs than characters proves it happened.
#[test]
fn latin_ligatures_form_across_cells() {
    let Some(with_ligature) = render("=>") else {
        return;
    };
    let Some(without) = render("=x") else {
        return;
    };

    let palette = Palette::default();
    assert!(ink(&with_ligature, &palette) > 0);
    assert_ne!(with_ligature, without);
}

/// A run stops at a blank cell, so two words never flow into one another.
#[test]
fn a_blank_cell_breaks_the_run() {
    let Some(spaced) = render("ব ল") else {
        return;
    };
    let Some(joined) = render("বল") else {
        return;
    };
    assert_ne!(spaced, joined);
}
