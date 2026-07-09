//! A terminal bound to a pseudoterminal: bytes in, screen state out.

use std::io::{Read, Write};
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError, channel};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use bab_vt::Terminal;

use crate::pty::{Command, Pty, Size};

/// Read chunk size. Large enough that a burst of output is a few syscalls.
const READ_BUFFER: usize = 64 * 1024;

/// A running shell and the terminal state it drives.
///
/// Output is read on a background thread and applied by [`Session::pump`], so the
/// caller controls when screen state changes. A UI pumps once per frame.
#[derive(Debug)]
pub struct Session {
    pty: Pty,
    terminal: Terminal,
    output: Receiver<Vec<u8>>,
    /// Set when the reader thread sees EOF, which means the child is gone.
    closed: bool,
}

impl Session {
    /// Spawn `command` on a new pseudoterminal sized to `size`.
    pub fn spawn(command: Command, size: Size) -> Result<Self> {
        let pty = Pty::spawn(command, size)?;
        let mut reader = pty.reader()?;
        let (sender, output) = channel();

        thread::Builder::new()
            .name("bab-pty-reader".into())
            .spawn(move || {
                let mut buffer = vec![0_u8; READ_BUFFER];
                loop {
                    match reader.read(&mut buffer) {
                        // EOF: the child exited and closed the slave side.
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if sender.send(buffer[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })?;

        Ok(Self {
            pty,
            terminal: Terminal::new(size.rows as usize, size.cols as usize),
            output,
            closed: false,
        })
    }

    #[must_use]
    pub const fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// Whether the child has exited and all its output has been applied.
    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    /// Apply all output available right now. Returns whether anything was applied.
    pub fn pump(&mut self) -> Result<bool> {
        self.drain(false)
    }

    /// Wait up to `timeout` for output, then apply everything available.
    pub fn pump_timeout(&mut self, timeout: Duration) -> Result<bool> {
        match self.output.recv_timeout(timeout) {
            Ok(chunk) => {
                self.terminal.feed(&chunk);
                self.drain(true)
            }
            Err(RecvTimeoutError::Timeout) => Ok(false),
            Err(RecvTimeoutError::Disconnected) => {
                self.closed = true;
                Ok(false)
            }
        }
    }

    /// Apply every pending chunk, then flush replies exactly once.
    ///
    /// `seeded` says a chunk was already fed by the caller. Flushing is tied to
    /// whether anything was fed at all, never to whether this call found new bytes —
    /// otherwise a reply produced by the seeded chunk is generated and dropped, and
    /// the application that queried the terminal blocks forever.
    fn drain(&mut self, seeded: bool) -> Result<bool> {
        let mut applied = seeded;
        loop {
            match self.output.try_recv() {
                Ok(chunk) => {
                    self.terminal.feed(&chunk);
                    applied = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.closed = true;
                    break;
                }
            }
        }

        if applied {
            self.flush_replies()?;
        }
        Ok(applied)
    }

    /// Send input to the child, as a keypress would.
    pub fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.pty.write_all(bytes)?;
        self.pty.flush()?;
        Ok(())
    }

    /// Send pasted text, bracketed when the application asked for it.
    ///
    /// Without brackets an application cannot distinguish a paste from typing, so a
    /// pasted newline runs a command. Bracketing is what makes paste safe in a shell.
    pub fn paste(&mut self, text: &str) -> Result<()> {
        if self.terminal.modes().bracketed_paste {
            self.send(b"\x1b[200~")?;
            self.send(text.as_bytes())?;
            self.send(b"\x1b[201~")
        } else {
            self.send(text.as_bytes())
        }
    }

    /// Resize the screen and tell the child, raising `SIGWINCH`.
    pub fn resize(&mut self, size: Size) -> Result<()> {
        self.terminal.resize(size.rows as usize, size.cols as usize);
        self.pty.resize(size)
    }

    pub fn kill(&mut self) -> Result<()> {
        self.pty.kill()
    }

    /// Write back anything the terminal owes the child, such as query replies.
    fn flush_replies(&mut self) -> Result<()> {
        let replies = self.terminal.take_output();
        if !replies.is_empty() {
            self.send(&replies)?;
        }
        Ok(())
    }
}
