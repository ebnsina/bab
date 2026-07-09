//! The pieces a native shell drives: a shell, a screen, and a renderer.

use std::ffi::CString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use bab_input::{Key, Modifiers, keyboard};
use bab_pty::{Command, Session, Size};
use bab_render::{CursorState, Renderer};
use bab_text::{Face, FontStack};

/// Fonts are embedded so the binary never depends on where it was installed from,
/// and so the Bengali fallback can never go missing on a user's machine.
const FIRA_CODE: &[u8] = include_bytes!("../../../assets/fonts/FiraCodeNerdFontMono-Regular.ttf");
const NOTO_BENGALI: &[u8] = include_bytes!("../../../assets/fonts/NotoSansBengali-Regular.ttf");

/// Font size in points. Physical pixels are this times the display's scale factor.
const FONT_POINTS: f32 = 14.0;

/// Inset from the window edge, in points. Text flush against the frame looks unfinished.
const PADDING_POINTS: f32 = 12.0;

/// How opaque the window is. Slightly translucent, to sit on the vibrancy behind it.
const BACKGROUND_ALPHA: f32 = 0.92;

/// Half of a full blink. macOS uses roughly this, and matching it makes `bab` feel
/// like it belongs on the system rather than merely running on it.
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// A terminal bound to one platform surface.
#[derive(Debug)]
pub struct Terminal {
    session: Session,
    renderer: Renderer,
    focused: bool,
    /// When the cursor last moved or the user last typed. Blinking restarts from
    /// solid on every keystroke, so the cursor is never invisible while you type.
    blink_epoch: Instant,
    /// Owned so the C API can hand out a pointer that stays valid until the next call.
    title: CString,
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
        scale: f32,
    ) -> Result<Self> {
        let fonts = FontStack::new(vec![
            Face::new("Fira Code Nerd Font Mono", Arc::new(FIRA_CODE.to_vec()))?,
            Face::new("Noto Sans Bengali", Arc::new(NOTO_BENGALI.to_vec()))?,
        ])?;

        // SAFETY: forwarded from the caller.
        let mut renderer = unsafe {
            Renderer::new_for_metal_layer(layer, width, height, fonts, font_pixels(scale))?
        };
        renderer.set_padding(PADDING_POINTS * scale, PADDING_POINTS * scale);

        let mut palette = renderer.palette();
        palette.set_background_alpha(BACKGROUND_ALPHA);
        renderer.set_palette(palette);

        let size = grid_size(&renderer, width, height);
        let session = Session::spawn(Command::default(), size)?;

        Ok(Self {
            session,
            renderer,
            focused: true,
            blink_epoch: Instant::now(),
            title: CString::default(),
        })
    }

    /// Apply pending output and draw one frame. Returns whether the shell is alive.
    pub fn frame(&mut self) -> Result<bool> {
        if self.session.pump()? {
            // New output usually means the cursor moved. Restart the blink solid.
            self.blink_epoch = Instant::now();
        }
        if self.session.is_closed() {
            return Ok(false);
        }

        let focused = self.focused;
        let blink_on = self.blink_phase();
        let terminal = self.session.terminal();
        let modes = terminal.modes();

        let cursor = modes.cursor_visible.then(|| CursorState {
            position: terminal.grid().cursor(),
            style: modes.cursor_style,
            focused,
            // An unfocused cursor never blinks: it must not pull the eye to a window
            // that is not listening.
            visible: !focused || !modes.cursor_style.blink || blink_on,
        });

        self.renderer.render(terminal.grid(), cursor)?;
        Ok(true)
    }

    /// Whether the cursor is in the lit half of its blink.
    fn blink_phase(&self) -> bool {
        let elapsed = self.blink_epoch.elapsed().as_millis();
        let interval = BLINK_INTERVAL.as_millis();
        (elapsed / interval).is_multiple_of(2)
    }

    /// The title the running application asked for, or the default. Never null.
    pub fn title(&mut self) -> &CString {
        let wanted = self.session.terminal().title().unwrap_or("bab");
        if self.title.to_str() != Ok(wanted) {
            self.title = CString::new(wanted).unwrap_or_default();
        }
        &self.title
    }

    /// Resize to a new pixel size, telling the shell about its new cell count.
    ///
    /// `scale` can change when a window moves between displays, so the font is
    /// remeasured every time rather than only at startup.
    pub fn resize(&mut self, width: u32, height: u32, scale: f32) -> Result<()> {
        self.renderer.set_font_size(font_pixels(scale))?;
        self.renderer
            .set_padding(PADDING_POINTS * scale, PADDING_POINTS * scale);
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
                        self.blink_epoch = Instant::now();
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
            self.blink_epoch = Instant::now();
            self.session.send(&bytes)?;
        }
        Ok(())
    }

    /// Paste text, bracketed when the application asked for it.
    pub fn paste(&mut self, text: &str) -> Result<()> {
        self.session.paste(text)
    }
}

/// The font size in physical pixels for a display of the given scale.
///
/// Sizes cross the platform boundary in pixels, so the font must be scaled too.
/// Leaving it in points renders the text at half size on a Retina display.
fn font_pixels(scale: f32) -> f32 {
    FONT_POINTS * scale.max(1.0)
}

/// How many whole cells fit in a pixel rectangle. Never zero: a zero-sized pty is
/// invalid, and applications divide by the cell count.
fn grid_size(renderer: &Renderer, width: u32, height: u32) -> Size {
    let cell = renderer.metrics().cell;
    let [pad_x, pad_y] = renderer.padding();

    // Padding eats into the drawable area, so the shell gets fewer cells than the
    // window would otherwise hold. Forgetting this hides the last row under the edge.
    let usable_width = (width as f32 - pad_x * 2.0).max(cell.width);
    let usable_height = (height as f32 - pad_y * 2.0).max(cell.height);

    let cols = (usable_width / cell.width).floor().max(1.0);
    let rows = (usable_height / cell.height).floor().max(1.0);
    Size::new(rows as u16, cols as u16)
}
