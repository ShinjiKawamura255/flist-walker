use anyhow::{Context, Result};
use crossterm::cursor::{Hide, Show};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use std::io::{self, Write};

pub(super) trait TerminalOps {
    fn enable_raw_mode(&mut self) -> io::Result<()>;
    fn disable_raw_mode(&mut self) -> io::Result<()>;
    fn enter_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn leave_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn hide_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn show_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn enable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
    fn disable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()>;
}

pub(super) struct CrosstermOps;

impl TerminalOps for CrosstermOps {
    fn enable_raw_mode(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()
    }

    fn disable_raw_mode(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()
    }

    fn enter_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, EnterAlternateScreen)
    }

    fn leave_alternate<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, LeaveAlternateScreen)
    }

    fn hide_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, Hide)
    }

    fn show_cursor<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, Show)
    }

    fn enable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, EnableBracketedPaste)
    }

    fn disable_paste<W: Write>(&mut self, writer: &mut W) -> io::Result<()> {
        execute!(writer, DisableBracketedPaste)
    }
}

pub(super) struct TerminalGuard<O: TerminalOps, W: Write> {
    pub(super) ops: O,
    pub(super) writer: W,
    pub(super) raw_mode: bool,
    pub(super) alternate_screen: bool,
    pub(super) cursor_hidden: bool,
    pub(super) bracketed_paste: bool,
}

impl<O: TerminalOps, W: Write> TerminalGuard<O, W> {
    pub(super) fn start(ops: O, writer: W) -> Result<Self> {
        let mut guard = Self {
            ops,
            writer,
            raw_mode: false,
            alternate_screen: false,
            cursor_hidden: false,
            bracketed_paste: false,
        };
        guard
            .ops
            .enable_raw_mode()
            .context("failed to enable terminal raw mode")?;
        guard.raw_mode = true;
        guard
            .ops
            .enter_alternate(&mut guard.writer)
            .context("failed to enter alternate screen")?;
        guard.alternate_screen = true;
        guard
            .ops
            .hide_cursor(&mut guard.writer)
            .context("failed to hide terminal cursor")?;
        guard.cursor_hidden = true;
        guard
            .ops
            .enable_paste(&mut guard.writer)
            .context("failed to enable bracketed paste")?;
        guard.bracketed_paste = true;
        Ok(guard)
    }

    pub(super) fn writer_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

impl<O: TerminalOps, W: Write> Drop for TerminalGuard<O, W> {
    fn drop(&mut self) {
        if self.bracketed_paste {
            let _ = self.ops.disable_paste(&mut self.writer);
            self.bracketed_paste = false;
        }
        if self.cursor_hidden {
            let _ = self.ops.show_cursor(&mut self.writer);
            self.cursor_hidden = false;
        }
        if self.alternate_screen {
            let _ = self.ops.leave_alternate(&mut self.writer);
            self.alternate_screen = false;
        }
        if self.raw_mode {
            let _ = self.ops.disable_raw_mode();
            self.raw_mode = false;
        }
    }
}

pub(super) fn run_terminal_operation<O, W, T, F>(
    mut guard: TerminalGuard<O, W>,
    operation: F,
) -> Result<T>
where
    O: TerminalOps,
    W: Write,
    F: FnOnce(&mut W) -> Result<T>,
{
    let result = operation(guard.writer_mut());
    drop(guard);
    result
}
