//! The pieces a native shell drives: a shell, a screen, and a renderer.

use std::ffi::CString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use bab_config::{Config, CursorShape as ConfigCursorShape};
use bab_input::mouse::{MouseButton, MouseEvent, MouseEventKind};
use bab_input::{Key, Modifiers, keyboard, mouse};
use bab_pty::{Command, Session, Size};
use bab_render::{CursorState, Palette, Renderer};
use bab_text::{Face, FontStack};
use bab_vt::{Cursor, CursorShape, CursorStyle, MouseTracking, Selection, SelectionMode};

/// Fonts are embedded so the binary never depends on where it was installed from,
/// and so the Bengali fallback can never go missing on a user's machine.
const JETBRAINS_MONO: &[u8] =
    include_bytes!("../../../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
const NOTO_BENGALI: &[u8] = include_bytes!("../../../assets/fonts/NotoSansBengali-Regular.ttf");

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
    config: Config,
    /// Owned so the C API can hand out a pointer that stays valid until the next call.
    title: CString,
    selected: CString,
    selection: Option<Selection>,
    dragging: bool,
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
        let (config, warning) = match Config::default_path() {
            Some(path) => Config::load(&path),
            None => (Config::default(), None),
        };
        if let Some(warning) = warning {
            eprintln!("bab: ignoring config: {warning}");
        }

        let fonts = load_fonts(&config)?;

        // SAFETY: forwarded from the caller.
        let mut renderer = unsafe {
            Renderer::new_for_metal_layer(layer, width, height, fonts, config.font.size * scale)?
        };
        apply_appearance(&mut renderer, &config, scale);

        let size = grid_size(&renderer, width, height);
        let mut session = Session::spawn(Command::default(), size)?;
        session
            .terminal_mut()
            .set_cursor_style(cursor_style(&config));

        Ok(Self {
            session,
            renderer,
            focused: true,
            blink_epoch: Instant::now(),
            config,
            title: CString::default(),
            selected: CString::default(),
            selection: None,
            dragging: false,
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
        let selection = self.selection;
        let terminal = self.session.terminal();
        let modes = terminal.modes();

        // A cursor drawn over history would point at a line the shell is not editing.
        let at_bottom = terminal.grid().is_at_bottom();
        let cursor = (modes.cursor_visible && at_bottom).then(|| CursorState {
            position: terminal.grid().cursor(),
            style: modes.cursor_style,
            focused,
            // An unfocused cursor never blinks: it must not pull the eye to a window
            // that is not listening.
            visible: !focused || !modes.cursor_style.blink || blink_on,
        });

        self.renderer
            .render_with_selection(terminal.grid(), cursor, selection.as_ref())?;
        Ok(true)
    }

    /// Handle a mouse event at a pixel position inside the view.
    ///
    /// An application that asked for mouse reporting gets the event, and holding shift
    /// overrides that so a user can always select text — which is what every terminal
    /// does, and what people reach for without thinking.
    pub fn mouse(
        &mut self,
        kind: MouseEventKind,
        button: Option<MouseButton>,
        x: f32,
        y: f32,
        modifiers: Modifiers,
        clicks: u32,
    ) -> Result<()> {
        let position = self.cell_at(x, y);
        let modes = *self.session.terminal().modes();

        let reporting = modes.mouse_tracking != MouseTracking::Off;
        if reporting && !modifiers.contains(Modifiers::SHIFT) {
            let event = MouseEvent {
                kind,
                button,
                row: position.row,
                col: position.col,
                modifiers,
            };
            if let Some(bytes) = mouse::encode(event, &modes) {
                self.session.send(&bytes)?;
            }
            return Ok(());
        }

        match kind {
            MouseEventKind::Press => {
                self.dragging = true;
                self.selection = Some(Selection::new(
                    position,
                    SelectionMode::from_click_count(clicks.max(1)),
                ));
            }
            MouseEventKind::Motion if self.dragging => {
                if let Some(selection) = &mut self.selection {
                    selection.drag_to(position);
                }
            }
            MouseEventKind::Motion => {}
            MouseEventKind::Release => self.dragging = false,
        }
        Ok(())
    }

    /// Scroll the viewport. Positive scrolls back into history.
    ///
    /// A full-screen application owns the alternate screen and keeps no history, so
    /// the wheel becomes arrow keys there — which is what makes scrolling work inside
    /// `less` and `man` without either side knowing about the other.
    pub fn scroll(&mut self, lines: i32) -> Result<()> {
        if lines == 0 {
            return Ok(());
        }
        let modes = *self.session.terminal().modes();

        if modes.alt_screen {
            if modes.mouse_tracking != MouseTracking::Off {
                return Ok(());
            }
            let key = if lines > 0 { Key::Up } else { Key::Down };
            for _ in 0..lines.unsigned_abs() {
                if let Some(bytes) = keyboard::encode(&key, Modifiers::NONE, &modes) {
                    self.session.send(&bytes)?;
                }
            }
            return Ok(());
        }

        let grid = self.session.terminal_mut().grid_mut();
        if lines > 0 {
            grid.scroll_back(lines.unsigned_abs() as usize);
        } else {
            grid.scroll_forward(lines.unsigned_abs() as usize);
        }
        Ok(())
    }

    /// Jump the viewport back to the live screen, as typing does.
    pub fn scroll_to_bottom(&mut self) {
        self.session.terminal_mut().grid_mut().scroll_to_bottom();
    }

    /// The selected text, or empty. Never null.
    pub fn selection_text(&mut self) -> &CString {
        let text = self
            .selection
            .as_ref()
            .map(|selection| selection.text(self.session.terminal().grid()))
            .unwrap_or_default();

        if self.selected.to_str() != Ok(text.as_str()) {
            self.selected = CString::new(text).unwrap_or_default();
        }
        &self.selected
    }

    pub const fn clear_selection(&mut self) {
        self.selection = None;
        self.dragging = false;
    }

    /// The cell under a pixel position, clamped into the grid.
    fn cell_at(&self, x: f32, y: f32) -> Cursor {
        let cell = self.renderer.metrics().cell;
        let [pad_x, pad_y] = self.renderer.padding();
        let grid = self.session.terminal().grid();

        let col = ((x - pad_x) / cell.width).floor().max(0.0) as usize;
        let row = ((y - pad_y) / cell.height).floor().max(0.0) as usize;

        Cursor {
            row: row.min(grid.rows().saturating_sub(1)),
            col: col.min(grid.cols().saturating_sub(1)),
        }
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
        self.renderer.set_font_size(self.config.font.size * scale)?;
        let padding = self.config.window.padding * scale;
        self.renderer.set_padding(padding, padding);
        self.renderer.resize(width, height);
        let size = grid_size(&self.renderer, width, height);
        self.session.resize(size)
    }

    /// The height of one cell, in physical pixels. A host converting a wheel delta
    /// into a line count needs this; guessing it makes trackpad scrolling feel wrong.
    #[must_use]
    pub fn cell_height(&self) -> f32 {
        self.renderer.metrics().cell.height
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
                        self.clear_selection();
                        self.scroll_to_bottom();
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
            self.clear_selection();
            // Typing means you want to see what you are typing.
            self.scroll_to_bottom();
            self.session.send(&bytes)?;
        }
        Ok(())
    }

    /// Paste text, bracketed when the application asked for it.
    pub fn paste(&mut self, text: &str) -> Result<()> {
        self.session.paste(text)
    }
}

/// Load the configured faces, falling back to the bundled ones.
///
/// A configured face that cannot be read is reported and replaced, never fatal. A font
/// file that moved must not stop the terminal from opening — the user needs a terminal
/// to fix the config in.
fn load_fonts(config: &Config) -> Result<FontStack> {
    let bundled_primary = || {
        Face::new(
            "JetBrains Mono Nerd Font Mono",
            Arc::new(JETBRAINS_MONO.to_vec()),
        )
    };
    let bundled_fallback = || Face::new("Noto Sans Bengali", Arc::new(NOTO_BENGALI.to_vec()));

    let primary = match &config.font.file {
        Some(path) => match load_face(path) {
            Ok(face) => face,
            Err(error) => {
                eprintln!("bab: falling back to the bundled font: {error:#}");
                bundled_primary()?
            }
        },
        None => bundled_primary()?,
    };
    let mut faces = vec![primary];

    match &config.font.fallback {
        Some(paths) => {
            for path in paths {
                match load_face(path) {
                    Ok(face) => faces.push(face),
                    Err(error) => eprintln!("bab: skipping fallback font: {error:#}"),
                }
            }
            // Without a Bengali face the script this project exists for renders as tofu.
            if faces.len() == 1 {
                eprintln!("bab: no fallback font loaded; adding the bundled Bengali face");
                faces.push(bundled_fallback()?);
            }
        }
        None => faces.push(bundled_fallback()?),
    }

    FontStack::new(faces)
}

fn load_face(path: &std::path::Path) -> Result<Face> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Face::new(name, Arc::new(bytes))
}

fn apply_appearance(renderer: &mut Renderer, config: &Config, scale: f32) {
    let padding = config.window.padding * scale;
    renderer.set_padding(padding, padding);

    let colors = &config.colors;
    let mut palette = Palette {
        foreground: colors.foreground.to_rgba(1.0),
        background: colors.background.to_rgba(config.window.opacity),
        accent: colors.accent.to_rgba(1.0),
        selection: colors.selection.to_rgba(colors.selection_alpha),
        ansi: [[0.0; 4]; 16],
    };
    for (slot, color) in palette.ansi.iter_mut().zip(&colors.ansi) {
        *slot = color.to_rgba(1.0);
    }
    renderer.set_palette(palette);
}

const fn cursor_style(config: &Config) -> CursorStyle {
    let shape = match config.cursor.shape {
        ConfigCursorShape::Block => CursorShape::Block,
        ConfigCursorShape::Underline => CursorShape::Underline,
        ConfigCursorShape::Bar => CursorShape::Bar,
    };
    CursorStyle {
        shape,
        blink: config.cursor.blink,
    }
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
