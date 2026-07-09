//! Spawning and driving a pseudoterminal.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// How to launch the child process.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Command {
    /// The program to run. Defaults to the user's login shell.
    pub program: Option<OsString>,
    pub args: Vec<OsString>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
}

impl Command {
    fn build(self) -> CommandBuilder {
        let mut builder = match self.program {
            Some(program) => CommandBuilder::new(program),
            None => CommandBuilder::new_default_prog(),
        };

        builder.args(self.args);
        if let Some(cwd) = self.cwd {
            builder.cwd(cwd);
        }

        // Until `bab`'s own terminfo entry is published and installed on remote
        // hosts, claiming an unknown TERM breaks key handling over SSH.
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");
        for (key, value) in self.env {
            builder.env(key, value);
        }

        builder
    }
}

/// The visible size of the terminal, in cells.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Size {
    pub rows: u16,
    pub cols: u16,
}

impl Size {
    #[must_use]
    pub const fn new(rows: u16, cols: u16) -> Self {
        Self { rows, cols }
    }
}

impl From<Size> for PtySize {
    fn from(size: Size) -> Self {
        Self {
            rows: size.rows,
            cols: size.cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

/// A pseudoterminal with a child process attached to its slave side.
pub struct Pty {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl std::fmt::Debug for Pty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pty").finish_non_exhaustive()
    }
}

impl Pty {
    /// Open a pseudoterminal and spawn `command` on its slave side.
    pub fn spawn(command: Command, size: Size) -> Result<Self> {
        let pair = native_pty_system()
            .openpty(size.into())
            .context("failed to open pty")?;

        let child = pair
            .slave
            .spawn_command(command.build())
            .context("failed to spawn command")?;
        // Dropping the slave lets the master see EOF once the child exits. Holding
        // it open would make reads block forever after the shell is gone.
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;

        Ok(Self {
            master: pair.master,
            writer,
            child,
        })
    }

    /// A new handle for reading the child's output. Reads block.
    pub fn reader(&self) -> Result<Box<dyn Read + Send>> {
        self.master
            .try_clone_reader()
            .context("failed to clone pty reader")
    }

    /// Tell the child its window changed. This is what raises `SIGWINCH`.
    pub fn resize(&self, size: Size) -> Result<()> {
        self.master
            .resize(size.into())
            .context("failed to resize pty")
    }

    /// Whether the child has exited.
    pub fn has_exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }

    pub fn kill(&mut self) -> Result<()> {
        self.child.kill().context("failed to kill child")
    }
}

impl Write for Pty {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
