//! The terminal: a [`Grid`] driven by a VT escape-sequence parser.

use std::fmt;

use vte::{Params, Perform};

use crate::grid::{Grid, LineErase, ScreenErase};
use crate::sgr;

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
                grid: Grid::new(rows, cols),
                title: None,
            },
        }
    }

    /// Feed bytes read from the PTY.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.state, bytes);
    }

    #[must_use]
    pub const fn grid(&self) -> &Grid {
        &self.state.grid
    }

    /// The title most recently set via `OSC 0` or `OSC 2`.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.state.title.as_deref()
    }
}

#[derive(Debug)]
struct State {
    grid: Grid,
    title: Option<String>,
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

impl Perform for State {
    fn print(&mut self, c: char) {
        self.grid.print(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x08 => self.grid.backspace(),
            0x09 => self.grid.tab(),
            0x0a..=0x0c => self.grid.linefeed(),
            0x0d => self.grid.carriage_return(),
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        if ignore || !intermediates.is_empty() {
            return;
        }

        match action {
            'A' => self.grid.move_up(arg_or(params, 1)),
            'B' => self.grid.move_down(arg_or(params, 1)),
            'C' => self.grid.move_right(arg_or(params, 1)),
            'D' => self.grid.move_left(arg_or(params, 1)),
            'G' => {
                let col = arg_or(params, 1) - 1;
                let row = self.grid.cursor().row;
                self.grid.goto(row, col);
            }
            'd' => {
                let row = arg_or(params, 1) - 1;
                let col = self.grid.cursor().col;
                self.grid.goto(row, col);
            }
            'H' | 'f' => {
                let mut iter = params.iter();
                let row = iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1)
                    .max(1);
                let col = iter
                    .next()
                    .and_then(|p| p.first().copied())
                    .unwrap_or(1)
                    .max(1);
                self.grid.goto(usize::from(row) - 1, usize::from(col) - 1);
            }
            'J' => {
                let mode = match arg_or(params, 0) {
                    1 => ScreenErase::Above,
                    2 | 3 => ScreenErase::All,
                    _ => ScreenErase::Below,
                };
                self.grid.erase_screen(mode);
            }
            'K' => {
                let mode = match arg_or(params, 0) {
                    1 => LineErase::ToStart,
                    2 => LineErase::All,
                    _ => LineErase::ToEnd,
                };
                self.grid.erase_line(mode);
            }
            'm' => sgr::apply(self.grid.attrs_mut(), params),
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
