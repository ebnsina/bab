//! Run a real shell on a pseudoterminal and render its output to a PNG.
//!
//! `cargo run -p bab-render --example shell -- out.png "ls -1"`

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bab_pty::{Command, Session, Size};
use bab_render::{CursorState, Renderer};
use bab_text::{Face, FontStack};

const ROWS: u16 = 10;
const COLS: u16 = 44;

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
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "shell.png".into());
    let script = args.next().unwrap_or_else(|| "ls -1".into());

    let command = Command {
        program: Some(OsString::from("/bin/sh")),
        args: vec![OsString::from("-c"), OsString::from(script)],
        ..Command::default()
    };
    let mut session = Session::spawn(command, Size::new(ROWS, COLS))?;

    // Drain the child until it exits or we give up.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !session.is_closed() && Instant::now() < deadline {
        session.pump_timeout(Duration::from_millis(50))?;
    }
    session.pump()?;

    let fonts = FontStack::new(vec![
        load("FiraCodeNerdFontMono-Regular.ttf"),
        load("NotoSansBengali-Regular.ttf"),
    ])?;
    let mut renderer = Renderer::new(16, 16, fonts, 18.0)?;
    let (width, height) = renderer.pixel_size(ROWS as usize, COLS as usize);
    renderer.resize(width, height);

    let terminal = session.terminal();
    renderer.render(
        terminal.grid(),
        Some(CursorState {
            position: terminal.grid().cursor(),
            style: terminal.modes().cursor_style,
            focused: true,
        }),
    )?;

    let pixels = renderer.read_pixels()?;
    let file = std::fs::File::create(&out)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&pixels)?;

    println!("wrote {out} ({width}x{height})");
    Ok(())
}
