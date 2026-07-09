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
        load("GeistMono-Regular.ttf"),
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

    renderer.render(terminal.grid()).expect("render");
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
    renderer.render(terminal.grid()).expect("render");

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
