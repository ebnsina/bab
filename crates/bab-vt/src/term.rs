//! The terminal: two screens driven by a VT escape-sequence parser.

use std::fmt;

use vte::{Params, Perform};

use crate::grid::{Grid, LineErase, ScreenErase};
use crate::modes::{CursorStyle, Mode, Modes, MouseTracking};
use crate::sgr;

/// Lines retained above the primary screen.
const SCROLLBACK_LIMIT: usize = 10_000;

/// Reported by `CSI c`: a VT220 with ANSI color.
const DEVICE_ATTRIBUTES: &[u8] = b"\x1b[?62;22c";

/// A terminal that consumes bytes from a PTY and maintains screen state.
pub struct Terminal {
    parser: vte::Parser,
    state: State,
}

impl fmt::Debug for Terminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `vte::Parser` is not `Debug`; its internal state is not interesting here.
        f.debug_struct("Terminal")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl Terminal {
    #[must_use]
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            parser: vte::Parser::new(),
            state: State {
                primary: Grid::new(rows, cols, SCROLLBACK_LIMIT),
                alt: Grid::new(rows, cols, 0),
                modes: Modes::default(),
                title: None,
                output: Vec::new(),
            },
        }
    }

    /// Feed bytes read from the PTY.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.state, bytes);
    }

    /// The active screen: the alternate one when a full-screen application owns it.
    #[must_use]
    pub const fn grid(&self) -> &Grid {
        self.state.grid()
    }

    #[must_use]
    pub const fn modes(&self) -> &Modes {
        &self.state.modes
    }

    /// The title most recently set via `OSC 0` or `OSC 2`.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.state.title.as_deref()
    }

    /// Take the bytes the terminal owes the PTY, such as query replies.
    ///
    /// The caller must write these back, or applications that query the terminal
    /// will hang waiting for a reply.
    #[must_use]
    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.state.output)
    }

    /// Resize both screens.
    pub fn resize(&mut self, rows: usize, cols: usize) {
        self.state.primary.resize(rows, cols);
        self.state.alt.resize(rows, cols);
    }
}

struct State {
    primary: Grid,
    alt: Grid,
    modes: Modes,
    title: Option<String>,
    output: Vec<u8>,
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State")
            .field("modes", &self.modes)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl State {
    const fn grid(&self) -> &Grid {
        if self.modes.alt_screen {
            &self.alt
        } else {
            &self.primary
        }
    }

    const fn grid_mut(&mut self) -> &mut Grid {
        if self.modes.alt_screen {
            &mut self.alt
        } else {
            &mut self.primary
        }
    }

    fn set_mode(&mut self, mode: Mode, enabled: bool) {
        match mode {
            Mode::ApplicationCursorKeys => self.modes.application_cursor_keys = enabled,
            Mode::Autowrap => {
                self.modes.autowrap = enabled;
                self.grid_mut().autowrap = enabled;
            }
            Mode::CursorVisible => self.modes.cursor_visible = enabled,
            Mode::AltScreen => self.set_alt_screen(enabled),
            Mode::BracketedPaste => self.modes.bracketed_paste = enabled,
            Mode::SynchronizedOutput => self.modes.synchronized_output = enabled,
            Mode::ColorSchemeUpdates => self.modes.color_scheme_updates = enabled,
            Mode::SgrMouse => self.modes.sgr_mouse = enabled,
            Mode::MouseTracking(kind) => {
                self.modes.mouse_tracking = if enabled { kind } else { MouseTracking::Off };
            }
        }
    }

    fn set_alt_screen(&mut self, enabled: bool) {
        if enabled == self.modes.alt_screen {
            return;
        }

        if enabled {
            self.primary.save_cursor();
            self.modes.alt_screen = true;
            self.alt.goto(0, 0);
            self.alt.erase_screen(ScreenErase::All);
        } else {
            self.modes.alt_screen = false;
            self.primary.restore_cursor();
        }
    }

    /// `DSR`. Report cursor position, one-indexed.
    fn report_cursor(&mut self) {
        let cursor = self.grid().cursor();
        let report = format!("\x1b[{};{}R", cursor.row + 1, cursor.col + 1);
        self.output.extend_from_slice(report.as_bytes());
    }
}

/// The first parameter, or `default` when absent or zero.
fn arg_or(params: &Params, default: u16) -> usize {
    let value = params
        .iter()
        .next()
        .and_then(|p| p.first().copied())
        .unwrap_or(0);
    usize::from(if value == 0 { default } else { value })
}

/// The `n`th parameter, or `default` when absent or zero.
fn arg_at(params: &Params, index: usize, default: u16) -> usize {
    let value = params
        .iter()
        .nth(index)
        .and_then(|p| p.first().copied())
        .unwrap_or(0);
    usize::from(if value == 0 { default } else { value })
}

impl Perform for State {
    fn print(&mut self, c: char) {
        self.grid_mut().print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.grid_mut().backspace(),
            0x09 => self.grid_mut().tab(),
            0x0a..=0x0c => self.grid_mut().linefeed(),
            0x0d => self.grid_mut().carriage_return(),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore {
            return;
        }

        // `CSI ? … h/l` sets and resets DEC private modes.
        if intermediates == b"?" {
            if !matches!(action, 'h' | 'l') {
                return;
            }
            let enabled = action == 'h';
            for param in params.iter() {
                if let Some(&number) = param.first()
                    && let Some(mode) = Mode::from_number(number)
                {
                    self.set_mode(mode, enabled);
                }
            }
            return;
        }

        // `CSI Ps SP q` is DECSCUSR. The space is an intermediate, not a parameter.
        if intermediates == b" " {
            if action == 'q'
                && let Some(style) = CursorStyle::from_decscusr(arg_or(params, 0) as u16)
            {
                self.modes.cursor_style = style;
            }
            return;
        }

        if !intermediates.is_empty() {
            return;
        }

        match action {
            'A' => self.grid_mut().move_up(arg_or(params, 1)),
            'B' => self.grid_mut().move_down(arg_or(params, 1)),
            'C' => self.grid_mut().move_right(arg_or(params, 1)),
            'D' => self.grid_mut().move_left(arg_or(params, 1)),
            'G' => {
                let col = arg_or(params, 1) - 1;
                let row = self.grid().cursor().row;
                self.grid_mut().goto(row, col);
            }
            'd' => {
                let row = arg_or(params, 1) - 1;
                let col = self.grid().cursor().col;
                self.grid_mut().goto(row, col);
            }
            'H' | 'f' => {
                let row = arg_at(params, 0, 1) - 1;
                let col = arg_at(params, 1, 1) - 1;
                self.grid_mut().goto(row, col);
            }
            'J' => {
                let mode = match arg_or(params, 0) {
                    1 => ScreenErase::Above,
                    2 | 3 => ScreenErase::All,
                    _ => ScreenErase::Below,
                };
                self.grid_mut().erase_screen(mode);
            }
            'K' => {
                let mode = match arg_or(params, 0) {
                    1 => LineErase::ToStart,
                    2 => LineErase::All,
                    _ => LineErase::ToEnd,
                };
                self.grid_mut().erase_line(mode);
            }
            'L' => self.grid_mut().insert_lines(arg_or(params, 1)),
            'M' => self.grid_mut().delete_lines(arg_or(params, 1)),
            '@' => self.grid_mut().insert_chars(arg_or(params, 1)),
            'P' => self.grid_mut().delete_chars(arg_or(params, 1)),
            'X' => self.grid_mut().erase_chars(arg_or(params, 1)),
            'S' => self.grid_mut().scroll_up(arg_or(params, 1)),
            'T' => self.grid_mut().scroll_down(arg_or(params, 1)),
            'r' => {
                let rows = self.grid().rows();
                let top = arg_at(params, 0, 1) - 1;
                let bottom = arg_at(params, 1, rows as u16) - 1;
                self.grid_mut().set_scroll_region(top, bottom);
            }
            's' => self.grid_mut().save_cursor(),
            'u' => self.grid_mut().restore_cursor(),
            'm' => sgr::apply(self.grid_mut().attrs_mut(), params),
            'c' => self.output.extend_from_slice(DEVICE_ATTRIBUTES),
            'n' if arg_or(params, 0) == 6 => self.report_cursor(),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore || !intermediates.is_empty() {
            return;
        }
        match byte {
            b'7' => self.grid_mut().save_cursor(),
            b'8' => self.grid_mut().restore_cursor(),
            b'M' => self.grid_mut().reverse_linefeed(),
            b'D' => self.grid_mut().linefeed(),
            b'E' => {
                self.grid_mut().carriage_return();
                self.grid_mut().linefeed();
            }
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let [command, value, ..] = params else {
            return;
        };
        // OSC 0 sets icon name and title; OSC 2 sets the title alone.
        if matches!(*command, b"0" | b"2")
            && let Ok(title) = str::from_utf8(value)
        {
            self.title = Some(title.to_owned());
        }
    }
}
