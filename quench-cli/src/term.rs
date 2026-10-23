//! Terminal queries and screen control: is this a terminal, how big, and the escape sequences for the
//! alternate screen, clearing and cursor placement.

#![allow(unsafe_code)]

use std::io::{self, Write};

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
    // SAFETY: `isatty` only inspects the descriptor.
    unsafe { libc::isatty(fd) == 1 }
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
    let mut ws = std::mem::MaybeUninit::<libc::winsize>::zeroed();
    // SAFETY: `TIOCGWINSZ` fills the `winsize` it is given.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, ws.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: zero-initialised, and written by the successful ioctl.
    let ws = unsafe { ws.assume_init() };
    if ws.ws_col == 0 || ws.ws_row == 0 {
        return Err(io::Error::other("the terminal reports no size"));
    }
    Ok((ws.ws_col, ws.ws_row))
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
    let echo_off = EchoOff::new(fd)?;
    let line = read_line(fd);
    drop(echo_off);
    if is_tty(fd) {
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

/// Reads bytes up to a newline straight from the descriptor, so nothing is left in a userspace buffer.
fn read_line(fd: i32) -> io::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = 0u8;
        // SAFETY: reads at most one byte into a local.
        let n = unsafe { libc::read(fd, (&raw mut byte).cast(), 1) };
        match n {
            0 => break,
            1 if byte == b'\n' => break,
            1 => bytes.push(byte),
            _ => {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
        }
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    String::from_utf8(bytes)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "input is not UTF-8"))
}

/// Echo switched off on a terminal until dropped; nothing happens for a descriptor that is not one.
struct EchoOff {
    fd: i32,
    original: Option<libc::termios>,
}

impl EchoOff {
    fn new(fd: i32) -> io::Result<Self> {
        if !is_tty(fd) {
            return Ok(Self { fd, original: None });
        }
        let mut attrs = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `tcgetattr` fills the struct on success.
        if unsafe { libc::tcgetattr(fd, attrs.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: initialised by the successful call above.
        let original = unsafe { attrs.assume_init() };
        let mut quiet = original;
        quiet.c_lflag &= !(libc::ECHO | libc::ECHOE | libc::ECHOK);
        quiet.c_lflag |= libc::ECHONL;
        // SAFETY: a valid termios derived from the kernel's own.
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw const quiet) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            fd,
            original: Some(original),
        })
    }
}

impl Drop for EchoOff {
    fn drop(&mut self) {
        if let Some(original) = &self.original {
            // SAFETY: restores the attributes saved in `new`.
            unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, original) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::RawFd;

    fn pty() -> (RawFd, RawFd) {
        let (mut master, mut slave) = (0, 0);
        // SAFETY: `openpty` writes two descriptors into the integers it is given.
        let rc = unsafe {
            libc::openpty(
                &raw mut master,
                &raw mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(rc, 0);
        (master, slave)
    }

    fn write_fd(fd: RawFd, bytes: &[u8]) {
        // SAFETY: a valid buffer and descriptor.
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(usize::try_from(n).unwrap(), bytes.len());
    }

    fn read_available(fd: RawFd) -> Vec<u8> {
        let mut pollfd = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let mut out = Vec::new();
        // SAFETY: one valid `pollfd`; the reads are into a local buffer.
        while unsafe { libc::poll(&raw mut pollfd, 1, 50) } > 0 {
            let mut buf = [0u8; 256];
            let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            out.extend_from_slice(&buf[..usize::try_from(n).unwrap()]);
        }
        out
    }

    fn close(fd: RawFd) {
        // SAFETY: a descriptor this test opened.
        unsafe { libc::close(fd) };
    }

    #[test]
    fn escape_sequences() {
        assert_eq!(move_to(0, 0), "\x1b[1;1H");
        assert_eq!(move_to(9, 4), "\x1b[5;10H");
        let mut out = Vec::new();
        clear_screen(&mut out).unwrap();
        assert_eq!(out, b"\x1b[2J\x1b[1;1H");
    }

    #[test]
    fn terminals_are_recognised_and_sized() {
        let (master, slave) = pty();
        assert!(is_tty(slave));
        assert!(!is_tty(-1));
        let ws = libc::winsize {
            ws_row: 30,
            ws_col: 100,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: sets the size of the pty this test owns.
        assert_eq!(
            unsafe { libc::ioctl(slave, libc::TIOCSWINSZ, &raw const ws) },
            0
        );
        assert_eq!(size_of(slave).unwrap(), (100, 30));
        close(master);
        close(slave);

        let mut fds = [0 as RawFd; 2];
        // SAFETY: `pipe` fills the array.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        assert!(size_of(fds[0]).is_err());
        close(fds[0]);
        close(fds[1]);
        // Whatever the test runner's stdio is, these must not panic.
        let _ = (stdin_is_tty(), stdout_is_tty(), size());
    }

    #[test]
    fn a_secret_is_read_without_echo() {
        let (master, slave) = pty();
        // Typed after the prompt is up, as a person would.
        let typist = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(150));
            write_fd(master, b"hunter2\r\n");
        });
        let secret = read_secret(slave, "Password: ").unwrap();
        typist.join().unwrap();
        assert_eq!(secret, "hunter2");
        let echoed = read_available(master);
        assert!(
            !String::from_utf8_lossy(&echoed).contains("hunter2"),
            "the password was echoed: {echoed:?}"
        );
        close(master);
        close(slave);
    }

    #[test]
    fn a_pipe_is_read_as_it_is() {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `pipe` fills the array.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        write_fd(fds[1], b"from a pipe\nnext line\n");
        assert_eq!(read_secret(fds[0], "").unwrap(), "from a pipe");
        assert_eq!(read_secret(fds[0], "").unwrap(), "next line");
        close(fds[1]);
        assert_eq!(read_secret(fds[0], "").unwrap(), "");
        close(fds[0]);

        // A closed descriptor is an error, not a hang.
        assert!(read_secret(-1, "").is_err());
    }

    #[test]
    fn invalid_utf8_is_an_error() {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `pipe` fills the array.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        write_fd(fds[1], b"\xff\xfe\n");
        assert_eq!(
            read_secret(fds[0], "").unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        close(fds[0]);
        close(fds[1]);
    }
}
