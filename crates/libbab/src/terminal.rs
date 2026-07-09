//! The pieces a native shell drives: a shell, a screen, and a renderer.

use std::sync::Arc;

use anyhow::Result;
use bab_input::{Key, Modifiers, keyboard};
use bab_pty::{Command, Session, Size};
use bab_render::{CursorState, Renderer};
use bab_text::{Face, FontStack};

/// Fonts are embedded so the binary never depends on where it was installed from,
/// and so the Bengali fallback can never go missing on a user's machine.
const GEIST_MONO: &[u8] = include_bytes!("../../../assets/fonts/GeistMono-Regular.ttf");
const NOTO_BENGALI: &[u8] = include_bytes!("../../../assets/fonts/NotoSansBengali-Regular.ttf");

const FONT_SIZE: f32 = 14.0;

/// A terminal bound to one platform surface.
#[derive(Debug)]
pub struct Terminal {
    session: Session,
    renderer: Renderer,
    focused: bool,
}

impl Terminal {
    /// Create a terminal drawing into `layer`, a `CAMetalLayer`.
    ///
    /// # Safety
    ///
    /// `layer` must be a valid `CAMetalLayer` that outlives this terminal.
    #[cfg(target_os = "macos")]
    pub unsafe fn new_for_metal_layer(
        layer: *mut std::ffi::c_void,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let fonts = FontStack::new(vec![
            Face::new("Geist Mono", Arc::new(GEIST_MONO.to_vec()))?,
            Face::new("Noto Sans Bengali", Arc::new(NOTO_BENGALI.to_vec()))?,
        ])?;

        // SAFETY: forwarded from the caller.
        let renderer =
            unsafe { Renderer::new_for_metal_layer(layer, width, height, fonts, FONT_SIZE)? };

        let size = grid_size(&renderer, width, height);
        let session = Session::spawn(Command::default(), size)?;

        Ok(Self {
            session,
            renderer,
            focused: true,
        })
    }

    /// Apply pending output and draw one frame. Returns whether the shell is alive.
    pub fn frame(&mut self) -> Result<bool> {
        self.session.pump()?;
        if self.session.is_closed() {
            return Ok(false);
        }

        let terminal = self.session.terminal();
        let cursor = terminal.modes().cursor_visible.then(|| CursorState {
            position: terminal.grid().cursor(),
            style: terminal.modes().cursor_style,
            focused: self.focused,
        });

        self.renderer.render(terminal.grid(), cursor)?;
        Ok(true)
    }

    /// Resize to a new pixel size, telling the shell about its new cell count.
    pub fn resize(&mut self, width: u32, height: u32) -> Result<()> {
        self.renderer.resize(width, height);
        let size = grid_size(&self.renderer, width, height);
        self.session.resize(size)
    }

    pub const fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Send a key press to the shell.
    ///
    /// `text` carries what the platform's input method produced, which is what an IME
    /// or a dead key hands us. It is sent verbatim unless a modifier changes the
    /// meaning, because re-encoding it character by character would drop the rest.
    pub fn key(&mut self, key: Option<Key>, text: &str, modifiers: Modifiers) -> Result<()> {
        let modes = *self.session.terminal().modes();

        let key = match key {
            Some(key) => key,
            None => {
                let plain =
                    !modifiers.contains(Modifiers::CONTROL) && !modifiers.contains(Modifiers::ALT);
                if plain {
                    if !text.is_empty() {
                        self.session.send(text.as_bytes())?;
                    }
                    return Ok(());
                }
                match text.chars().next() {
                    Some(c) => Key::Char(c),
                    None => return Ok(()),
                }
            }
        };

        if let Some(bytes) = keyboard::encode(&key, modifiers, &modes) {
            self.session.send(&bytes)?;
        }
        Ok(())
    }

    /// Paste text, bracketed when the application asked for it.
    pub fn paste(&mut self, text: &str) -> Result<()> {
        self.session.paste(text)
    }
}

/// How many whole cells fit in a pixel rectangle. Never zero: a zero-sized pty is
/// invalid, and applications divide by the cell count.
fn grid_size(renderer: &Renderer, width: u32, height: u32) -> Size {
    let cell = renderer.metrics().cell;
    let cols = (width as f32 / cell.width).floor().max(1.0);
    let rows = (height as f32 / cell.height).floor().max(1.0);
    Size::new(rows as u16, cols as u16)
}
