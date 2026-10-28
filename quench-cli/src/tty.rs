//! Raw terminal mode on Unix, through `stty`: [`RawMode`] switches a terminal to unbuffered, unechoed input and puts it back when dropped.
//!
//! The terminal's attributes are read and set by the system's `stty` on a handle to the descriptor, so
//! this crate needs no `libc` and no `unsafe`; the cost is a process per switch.

use std::fs::File;
use std::io::{self, IsTerminal};
use std::os::fd::AsFd;
use std::process::{Command, Stdio};

/// A handle that shares the open file of descriptor `fd`; `None` when there is no such descriptor.
pub(crate) fn open_fd(fd: i32) -> Option<File> {
    let owned = match fd {
        0 => io::stdin().as_fd().try_clone_to_owned(),
        1 => io::stdout().as_fd().try_clone_to_owned(),
        2 => io::stderr().as_fd().try_clone_to_owned(),
        n if n > 2 => return File::open(format!("/dev/fd/{n}")).ok(),
        _ => return None,
    };
    owned.ok().map(File::from)
}

/// `stty` with `args`, run on the terminal `tty`; what it prints.
pub(crate) fn stty(tty: &File, args: &[&str]) -> io::Result<String> {
    let output = Command::new("stty")
        .args(args)
        .stdin(tty.try_clone()?)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!("stty {} failed", args.join(" "))));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// The terminal on `fd` and its attributes as they were, in the form `stty` takes back; `None` when
/// `fd` is not a terminal.
pub(crate) fn save(fd: i32) -> io::Result<Option<(File, String)>> {
    let Some(tty) = open_fd(fd).filter(File::is_terminal) else {
        return Ok(None);
    };
    let saved = stty(&tty, &["-g"])?;
    Ok(Some((tty, saved)))
}

/// Puts a terminal in raw-ish mode (no echo, no line buffering, no signals
/// from the keyboard, no CR translation) and restores it on drop.
#[derive(Debug)]
pub struct RawMode {
    tty: File,
    saved: String,
}

impl RawMode {
    /// Enables raw mode on `fd`. `None` when `fd` is not a terminal.
    ///
    /// # Errors
    ///
    /// The error if `stty` cannot be run or refuses the terminal.
    pub fn enable(fd: i32) -> io::Result<Option<Self>> {
        let Some((tty, saved)) = save(fd)? else {
            return Ok(None);
        };
        stty(
            &tty,
            &[
                "-echo", "-icanon", "-isig", "-iexten", "-ixon", "-icrnl", "-inlcr", "-istrip",
                "min", "1", "time", "0",
            ],
        )?;
        Ok(Some(Self { tty, saved }))
    }

    /// Like [`enable`](Self::enable). Input typed before the switch (and while raw mode lasts) is kept
    /// either way, since `stty` does not flush; the name stays for the programs that poll for keys
    /// between other work.
    ///
    /// # Errors
    ///
    /// As [`enable`](Self::enable).
    pub fn enable_keeping_input(fd: i32) -> io::Result<Option<Self>> {
        Self::enable(fd)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // Nothing to do when it fails: the terminal is gone.
        let _ = stty(&self.tty, &[&self.saved]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    // A real terminal is needed to see the attributes change: `tests/pty.rs` runs them under one.
    #[quench_test::test]
    fn non_terminals_are_not_touched() {
        let (reader, _writer) = io::pipe().unwrap();
        assert!(RawMode::enable(reader.as_raw_fd()).unwrap().is_none());
        assert!(RawMode::enable_keeping_input(-1).unwrap().is_none());
        assert!(RawMode::enable(-1).unwrap().is_none());
        assert!(open_fd(-1).is_none() && open_fd(1 << 20).is_none());
        assert!(save(-1).unwrap().is_none());
    }
}
