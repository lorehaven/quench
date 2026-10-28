//! Terminal queries and screen control: is this a terminal, how big, and the escape sequences for the
//! alternate screen, clearing and cursor placement.

use crate::tty::{open_fd, save, stty};
use std::fs::File;
use std::io::{self, IsTerminal, Read, Write};

/// Switches to the alternate screen buffer (a full-screen program's own page).
pub const ENTER_ALTERNATE_SCREEN: &str = "\x1b[?1049h";
/// Back to the normal screen, restoring what was there.
pub const LEAVE_ALTERNATE_SCREEN: &str = "\x1b[?1049l";
/// Erases the whole screen (the cursor stays where it is).
pub const CLEAR_ALL: &str = "\x1b[2J";
/// Erases the cursor's line.
pub const CLEAR_LINE: &str = "\x1b[2K";
pub const HIDE_CURSOR: &str = "\x1b[?25l";
pub const SHOW_CURSOR: &str = "\x1b[?25h";

/// The sequence that puts the cursor at `column`, `row` (both 0-based, 0,0 is the top left).
#[must_use]
pub fn move_to(column: u16, row: u16) -> String {
    format!("\x1b[{};{}H", u32::from(row) + 1, u32::from(column) + 1)
}

/// Clears the screen and puts the cursor at the top left.
///
/// # Errors
///
/// When writing to `out` fails.
pub fn clear_screen(out: &mut impl Write) -> io::Result<()> {
    write!(out, "{CLEAR_ALL}{}", move_to(0, 0))?;
    out.flush()
}

/// Whether file descriptor `fd` is a terminal.
#[must_use]
pub fn is_tty(fd: i32) -> bool {
    open_fd(fd).is_some_and(|file| file.is_terminal())
}

#[must_use]
pub fn stdin_is_tty() -> bool {
    is_tty(0)
}

#[must_use]
pub fn stdout_is_tty() -> bool {
    is_tty(1)
}

/// `(columns, rows)` of the terminal on `fd`.
///
/// # Errors
///
/// When `fd` is not a terminal or reports no size.
pub fn size_of(fd: i32) -> io::Result<(u16, u16)> {
    let tty = open_fd(fd)
        .filter(File::is_terminal)
        .ok_or_else(|| io::Error::other("not a terminal"))?;
    // `stty size` prints "rows columns".
    let said = stty(&tty, &["size"])?;
    let mut numbers = said.split_whitespace().map(str::parse::<u16>);
    let (Some(Ok(rows)), Some(Ok(columns))) = (numbers.next(), numbers.next()) else {
        return Err(io::Error::other("the terminal reports no size"));
    };
    if rows == 0 || columns == 0 {
        return Err(io::Error::other("the terminal reports no size"));
    }
    Ok((columns, rows))
}

/// `(columns, rows)` of the terminal on standard output, falling back to standard error and input.
///
/// # Errors
///
/// When none of them is a terminal.
pub fn size() -> io::Result<(u16, u16)> {
    size_of(1).or_else(|_| size_of(2)).or_else(|_| size_of(0))
}

/// Prints `prompt` and reads a line from `fd` without echoing it (a password). A descriptor that is not a
/// terminal is read as it is. The trailing newline is not part of the result.
///
/// # Errors
///
/// When the terminal cannot be configured or the read fails.
pub fn read_secret(fd: i32, prompt: &str) -> io::Result<String> {
    let mut err = io::stderr();
    write!(err, "{prompt}")?;
    err.flush()?;
    let mut file = open_fd(fd).ok_or_else(|| io::Error::other("no such descriptor"))?;
    let echo_off = EchoOff::new(fd)?;
    let line = read_line(&mut file);
    let was_terminal = echo_off.is_some();
    drop(echo_off);
    if was_terminal {
        writeln!(err)?;
    }
    line
}

/// [`read_secret`] on standard input.
///
/// # Errors
///
/// As [`read_secret`].
pub fn prompt_password(prompt: &str) -> io::Result<String> {
    read_secret(0, prompt)
}

/// Reads bytes up to a newline straight from the file, so nothing is left in a userspace buffer.
fn read_line(file: &mut File) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match file.read(&mut byte) {
            Ok(0) => break,
            Ok(_) if byte[0] == b'\n' => break,
            Ok(_) => bytes.push(byte[0]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "input is not UTF-8"))
}

/// Echo switched off on a terminal until dropped.
struct EchoOff {
    tty: File,
    saved: String,
}

impl EchoOff {
    /// `None` for a descriptor that is not a terminal: nothing to switch.
    fn new(fd: i32) -> io::Result<Option<Self>> {
        let Some((tty, saved)) = save(fd)? else {
            return Ok(None);
        };
        stty(&tty, &["-echo", "-echoe", "-echok", "echonl"])?;
        Ok(Some(Self { tty, saved }))
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        // Nothing to do when it fails: the terminal is gone.
        let _ = stty(&self.tty, &[&self.saved]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

    // What needs a real terminal (size, the secret without echo) is tested under one in `tests/pty.rs`.
    #[quench_test::test]
    fn escape_sequences() {
        assert_eq!(move_to(0, 0), "\x1b[1;1H");
        assert_eq!(move_to(9, 4), "\x1b[5;10H");
        let mut out = Vec::new();
        clear_screen(&mut out).unwrap();
        assert_eq!(out, b"\x1b[2J\x1b[1;1H");
    }

    #[quench_test::test]
    fn a_pipe_is_not_a_terminal() {
        let (reader, _writer) = io::pipe().unwrap();
        assert!(!is_tty(reader.as_raw_fd()));
        assert!(!is_tty(-1));
        assert!(size_of(reader.as_raw_fd()).is_err());
        // Whatever the test runner's stdio is, these must not panic.
        let _ = (stdin_is_tty(), stdout_is_tty(), size());
    }

    #[quench_test::test]
    fn a_pipe_is_read_as_it_is() {
        let (reader, mut writer) = io::pipe().unwrap();
        writer.write_all(b"from a pipe\nnext line\n").unwrap();
        let fd = reader.as_raw_fd();
        assert_eq!(read_secret(fd, "").unwrap(), "from a pipe");
        assert_eq!(read_secret(fd, "").unwrap(), "next line");
        drop(writer);
        assert_eq!(read_secret(fd, "").unwrap(), "");

        // A closed descriptor is an error, not a hang.
        assert!(read_secret(-1, "").is_err());
    }

    #[quench_test::test]
    fn invalid_utf8_is_an_error() {
        let (reader, mut writer) = io::pipe().unwrap();
        writer.write_all(b"\xff\xfe\n").unwrap();
        assert_eq!(
            read_secret(reader.as_raw_fd(), "").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
