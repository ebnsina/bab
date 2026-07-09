//! Render a grid to a PNG so a human can look at it.
//!
//! `cargo run -p bab-render --example screenshot -- out.png`

use std::path::PathBuf;
use std::sync::Arc;

use bab_render::Renderer;
use bab_text::{Face, FontStack};
use bab_vt::Terminal;

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
    Face::new(file, Arc::new(std::fs::read(path).unwrap())).unwrap()
}

fn main() -> anyhow::Result<()> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "bab.png".into());
    let (width, height) = (560, 150);

    let fonts = FontStack::new(vec![
        load("FiraCodeNerdFontMono-Regular.ttf"),
        load("NotoSansBengali-Regular.ttf"),
    ])?;
    let mut renderer = Renderer::new(width, height, fonts, 18.0)?;
    renderer.set_padding(16.0, 14.0);

    let mut terminal = Terminal::new(6, 40);
    terminal.feed("\x1b[1;32mbab\x1b[0m \x1b[38;5;250m~ a terminal\x1b[0m\r\n".as_bytes());
    terminal.feed("প্রধানমন্ত্রী তারেক রহমান\r\n".as_bytes());
    terminal.feed("বাংলা ব্ল ক্ষ কি র্ক\r\n".as_bytes());
    terminal.feed("\x1b[7m reverse \x1b[0m \x1b[38;2;255;120;60mtruecolor\x1b[0m\r\n".as_bytes());
    terminal.feed("ligatures: => != -> <=>   icons: \u{e0b0} \u{f07c} \u{e702}\r\n".as_bytes());
    terminal.feed(b"\x1b[2 q$ ");

    renderer.render(
        terminal.grid(),
        Some(bab_render::CursorState {
            position: terminal.grid().cursor(),
            style: terminal.modes().cursor_style,
            focused: true,
            visible: true,
        }),
    )?;
    let pixels = renderer.read_pixels()?;

    let file = std::fs::File::create(&out)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;

    println!("wrote {out}");
    Ok(())
}
