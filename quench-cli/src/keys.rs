//! Key presses from a terminal in raw mode (the part of `crossterm::event` the tools use).
//!
//! [`parse_key`] turns the bytes a terminal sends into a [`KeyEvent`]; [`Keys`] reads them from a descriptor,
//! with a timeout so a program can do other work between keys. Escape sequences for arrows, Home/End,
//! Page Up/Down, Insert/Delete and F1-F12 are understood, with Shift/Alt/Ctrl modifiers.

#![allow(unsafe_code)]

use std::io;
use std::time::{Duration, Instant};

/// What was pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    /// F1 to F12.
    F(u8),
}

/// Held keys besides the one pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
    };

    /// From the xterm modifier parameter (1 plus a bit set of shift=1, alt=2, ctrl=4).
    const fn from_param(param: u32) -> Self {
        let bits = param.saturating_sub(1);
        Self {
            shift: bits & 1 != 0,
            alt: bits & 2 != 0,
            ctrl: bits & 4 != 0,
        }
    }
}

/// One key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: KeyCode,
    pub modifiers: Modifiers,
}

impl KeyEvent {
    const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            modifiers: Modifiers::NONE,
        }
    }

    const fn with(code: KeyCode, modifiers: Modifiers) -> Self {
        Self { code, modifiers }
    }
}

impl From<KeyCode> for KeyEvent {
    fn from(code: KeyCode) -> Self {
        Self::plain(code)
    }
}

/// The outcome of looking at the start of a byte buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    /// A key, and how many bytes it used.
    Key(KeyEvent, usize),
    /// The bytes so far are the start of a longer sequence.
    Incomplete,
    /// Bytes that mean nothing; skip this many.
    Skip(usize),
}

/// Reads one key from the start of `bytes`.
///
/// A lone `ESC` is reported as [`Parsed::Incomplete`] because it may begin a sequence; the reader decides it is
/// the Escape key after a short wait (see [`Keys::read`]).
#[must_use]
pub fn parse_key(bytes: &[u8]) -> Parsed {
    let Some(&first) = bytes.first() else {
        return Parsed::Incomplete;
    };
    match first {
        0x1b => parse_escape(bytes),
        b'\r' | b'\n' => Parsed::Key(KeyEvent::plain(KeyCode::Enter), 1),
        b'\t' => Parsed::Key(KeyEvent::plain(KeyCode::Tab), 1),
        0x08 | 0x7f => Parsed::Key(KeyEvent::plain(KeyCode::Backspace), 1),
        0x00 => Parsed::Key(
            KeyEvent::with(
                KeyCode::Char(' '),
                Modifiers {
                    ctrl: true,
                    ..Modifiers::NONE
                },
            ),
            1,
        ),
        0x01..=0x1a => Parsed::Key(
            KeyEvent::with(
                KeyCode::Char(char::from(first - 1 + b'a')),
                Modifiers {
                    ctrl: true,
                    ..Modifiers::NONE
                },
            ),
            1,
        ),
        0x1c..=0x1f => Parsed::Skip(1),
        _ => parse_char(bytes),
    }
}

/// A (possibly multi-byte) character.
fn parse_char(bytes: &[u8]) -> Parsed {
    let width = match bytes[0] {
        0x20..=0x7e => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return Parsed::Skip(1),
    };
    if bytes.len() < width {
        return Parsed::Incomplete;
    }
    std::str::from_utf8(&bytes[..width])
        .ok()
        .and_then(|s| s.chars().next())
        .map_or(Parsed::Skip(1), |c| {
            let shift = c.is_uppercase();
            Parsed::Key(
                KeyEvent::with(
                    KeyCode::Char(c),
                    Modifiers {
                        shift,
                        ..Modifiers::NONE
                    },
                ),
                width,
            )
        })
}

fn parse_escape(bytes: &[u8]) -> Parsed {
    let Some(&second) = bytes.get(1) else {
        return Parsed::Incomplete;
    };
    match second {
        b'[' => parse_csi(bytes),
        b'O' => {
            let Some(&third) = bytes.get(2) else {
                return Parsed::Incomplete;
            };
            let code = match third {
                b'A' => KeyCode::Up,
                b'B' => KeyCode::Down,
                b'C' => KeyCode::Right,
                b'D' => KeyCode::Left,
                b'H' => KeyCode::Home,
                b'F' => KeyCode::End,
                b'P' => KeyCode::F(1),
                b'Q' => KeyCode::F(2),
                b'R' => KeyCode::F(3),
                b'S' => KeyCode::F(4),
                _ => return Parsed::Skip(3),
            };
            Parsed::Key(KeyEvent::plain(code), 3)
        }
        0x1b => Parsed::Key(KeyEvent::plain(KeyCode::Esc), 1),
        _ => {
            // Alt plus whatever follows.
            match parse_key(&bytes[1..]) {
                Parsed::Key(mut event, used) => {
                    event.modifiers.alt = true;
                    Parsed::Key(event, used + 1)
                }
                Parsed::Incomplete => Parsed::Incomplete,
                Parsed::Skip(used) => Parsed::Skip(used + 1),
            }
        }
    }
}

/// `ESC [` sequences: `ESC [ A`, `ESC [ 1 ; 5 C`, `ESC [ 3 ~`, `ESC [ Z`.
fn parse_csi(bytes: &[u8]) -> Parsed {
    let mut index = 2;
    let mut params: Vec<u32> = Vec::new();
    let mut current: Option<u32> = None;
    loop {
        let Some(&b) = bytes.get(index) else {
            return Parsed::Incomplete;
        };
        index += 1;
        match b {
            b'0'..=b'9' => {
                current = Some(
                    current
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(u32::from(b - b'0')),
                );
            }
            b';' => {
                params.push(current.take().unwrap_or(0));
            }
            0x40..=0x7e => {
                params.push(current.take().unwrap_or(0));
                return finish_csi(b, &params, index);
            }
            _ => return Parsed::Skip(index),
        }
    }
}

fn finish_csi(final_byte: u8, params: &[u32], used: usize) -> Parsed {
    let modifiers = params.get(1).map_or(Modifiers::NONE, |&p| Modifiers::from_param(p));
    let first = params.first().copied().unwrap_or(0);
    let code = match final_byte {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'Z' => {
            return Parsed::Key(
                KeyEvent::with(
                    KeyCode::BackTab,
                    Modifiers {
                        shift: true,
                        ..Modifiers::NONE
                    },
                ),
                used,
            );
        }
        b'P' => KeyCode::F(1),
        b'Q' => KeyCode::F(2),
        b'R' => KeyCode::F(3),
        b'S' => KeyCode::F(4),
        b'~' => match first {
            1 | 7 => KeyCode::Home,
            2 => KeyCode::Insert,
            3 => KeyCode::Delete,
            4 | 8 => KeyCode::End,
            5 => KeyCode::PageUp,
            6 => KeyCode::PageDown,
            11..=15 => KeyCode::F(u8::try_from(first - 10).unwrap_or(1)),
            17..=21 => KeyCode::F(u8::try_from(first - 11).unwrap_or(1)),
            23 | 24 => KeyCode::F(u8::try_from(first - 12).unwrap_or(1)),
            _ => return Parsed::Skip(used),
        },
        _ => return Parsed::Skip(used),
    };
    Parsed::Key(KeyEvent::with(code, modifiers), used)
}

/// How long a lone `ESC` waits for the rest of a sequence before it is the Escape key.
const ESC_WAIT: Duration = Duration::from_millis(30);

/// A source of key presses: a descriptor in raw mode, usually standard input.
#[derive(Debug)]
pub struct Keys {
    fd: i32,
    pending: Vec<u8>,
}

impl Keys {
    /// Keys from standard input.
    #[must_use]
    pub const fn stdin() -> Self {
        Self::on_fd(0)
    }

    #[must_use]
    pub const fn on_fd(fd: i32) -> Self {
        Self {
            fd,
            pending: Vec::new(),
        }
    }

    /// Whether a key press is ready within `timeout` (zero checks without waiting).
    ///
    /// # Errors
    ///
    /// When the descriptor cannot be polled.
    pub fn poll(&mut self, timeout: Duration) -> io::Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if matches!(parse_key(&self.pending), Parsed::Key(..)) {
                return Ok(true);
            }
            if let Parsed::Skip(n) = parse_key(&self.pending) {
                self.pending.drain(..n);
                continue;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            // A lone ESC is a key of its own once nothing more arrives.
            if self.pending == [0x1b] && remaining.is_zero() {
                return Ok(true);
            }
            if !self.fill(remaining.min(if self.pending.is_empty() { remaining } else { ESC_WAIT }))? {
                if self.pending == [0x1b] {
                    return Ok(true);
                }
                if remaining.is_zero() {
                    return Ok(false);
                }
            }
        }
    }

    /// The next key press, waiting for it.
    ///
    /// # Errors
    ///
    /// [`io::ErrorKind::UnexpectedEof`] when the input has ended, or the read error.
    pub fn read(&mut self) -> io::Result<KeyEvent> {
        loop {
            match parse_key(&self.pending) {
                Parsed::Key(event, used) => {
                    self.pending.drain(..used);
                    return Ok(event);
                }
                Parsed::Skip(n) => {
                    self.pending.drain(..n);
                }
                Parsed::Incomplete => {
                    let wait = if self.pending.is_empty() {
                        Duration::from_secs(3600)
                    } else {
                        ESC_WAIT
                    };
                    if !self.fill(wait)? {
                        if self.pending == [0x1b] {
                            self.pending.clear();
                            return Ok(KeyEvent::plain(KeyCode::Esc));
                        }
                        if !self.pending.is_empty() {
                            // A sequence that never finished: drop it rather than wedge.
                            self.pending.clear();
                        }
                    }
                }
            }
        }
    }

    /// Waits up to `timeout` for input and appends it; `false` when none came.
    fn fill(&mut self, timeout: Duration) -> io::Result<bool> {
        let mut pollfd = libc::pollfd {
            fd: self.fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        // SAFETY: one valid `pollfd`.
        let ready = unsafe { libc::poll(&raw mut pollfd, 1, millis) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(false)
            } else {
                Err(error)
            };
        }
        if ready == 0 {
            return Ok(false);
        }
        let mut buf = [0u8; 64];
        // SAFETY: reads into a local buffer of the stated length.
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast(), buf.len()) };
        match usize::try_from(n) {
            Ok(0) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the terminal input has ended",
            )),
            Ok(n) => {
                self.pending.extend_from_slice(&buf[..n]);
                Ok(true)
            }
            Err(_) => {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    Ok(false)
                } else {
                    Err(error)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::RawFd;

    fn key(bytes: &[u8]) -> (KeyCode, Modifiers, usize) {
        match parse_key(bytes) {
            Parsed::Key(event, used) => (event.code, event.modifiers, used),
            other => panic!("{bytes:?} parsed as {other:?}"),
        }
    }

    const CTRL: Modifiers = Modifiers {
        ctrl: true,
        alt: false,
        shift: false,
    };
    const ALT: Modifiers = Modifiers {
        ctrl: false,
        alt: true,
        shift: false,
    };
    const SHIFT: Modifiers = Modifiers {
        ctrl: false,
        alt: false,
        shift: true,
    };

    #[test]
    fn plain_keys() {
        assert_eq!(key(b"a"), (KeyCode::Char('a'), Modifiers::NONE, 1));
        assert_eq!(key(b"Q"), (KeyCode::Char('Q'), SHIFT, 1));
        assert_eq!(key(b"\r"), (KeyCode::Enter, Modifiers::NONE, 1));
        assert_eq!(key(b"\n"), (KeyCode::Enter, Modifiers::NONE, 1));
        assert_eq!(key(b"\t"), (KeyCode::Tab, Modifiers::NONE, 1));
        assert_eq!(key(b"\x7f"), (KeyCode::Backspace, Modifiers::NONE, 1));
        assert_eq!(key(b"\x08"), (KeyCode::Backspace, Modifiers::NONE, 1));
        assert_eq!(key(b" "), (KeyCode::Char(' '), Modifiers::NONE, 1));
        assert_eq!(key("é".as_bytes()), (KeyCode::Char('é'), Modifiers::NONE, 2));
        assert_eq!(key("€x".as_bytes()), (KeyCode::Char('€'), Modifiers::NONE, 3));
        assert_eq!(key("😀".as_bytes()), (KeyCode::Char('😀'), Modifiers::NONE, 4));
    }

    #[test]
    fn control_and_alt_keys() {
        assert_eq!(key(b"\x01"), (KeyCode::Char('a'), CTRL, 1));
        assert_eq!(key(b"\x03"), (KeyCode::Char('c'), CTRL, 1));
        assert_eq!(key(b"\x1a"), (KeyCode::Char('z'), CTRL, 1));
        assert_eq!(key(b"\x00"), (KeyCode::Char(' '), CTRL, 1));
        assert_eq!(key(b"\x1bb"), (KeyCode::Char('b'), ALT, 2));
        assert_eq!(key(b"\x1b\r"), (KeyCode::Enter, ALT, 2));
        assert_eq!(key(b"\x1b\x1b"), (KeyCode::Esc, Modifiers::NONE, 1));
    }

    #[test]
    fn cursor_and_editing_keys() {
        for (bytes, code) in [
            (&b"\x1b[A"[..], KeyCode::Up),
            (b"\x1b[B", KeyCode::Down),
            (b"\x1b[C", KeyCode::Right),
            (b"\x1b[D", KeyCode::Left),
            (b"\x1b[H", KeyCode::Home),
            (b"\x1b[F", KeyCode::End),
            (b"\x1bOA", KeyCode::Up),
            (b"\x1bOH", KeyCode::Home),
            (b"\x1bOF", KeyCode::End),
            (b"\x1b[1~", KeyCode::Home),
            (b"\x1b[2~", KeyCode::Insert),
            (b"\x1b[3~", KeyCode::Delete),
            (b"\x1b[4~", KeyCode::End),
            (b"\x1b[5~", KeyCode::PageUp),
            (b"\x1b[6~", KeyCode::PageDown),
            (b"\x1b[7~", KeyCode::Home),
            (b"\x1b[8~", KeyCode::End),
        ] {
            assert_eq!(key(bytes), (code, Modifiers::NONE, bytes.len()), "{bytes:?}");
        }
        assert_eq!(key(b"\x1b[Z"), (KeyCode::BackTab, SHIFT, 3));
    }

    #[test]
    fn function_keys() {
        for (bytes, n) in [
            (&b"\x1bOP"[..], 1),
            (b"\x1bOQ", 2),
            (b"\x1bOR", 3),
            (b"\x1bOS", 4),
            (b"\x1b[11~", 1),
            (b"\x1b[15~", 5),
            (b"\x1b[17~", 6),
            (b"\x1b[21~", 10),
            (b"\x1b[23~", 11),
            (b"\x1b[24~", 12),
            (b"\x1b[1;2P", 1),
        ] {
            assert_eq!(key(bytes).0, KeyCode::F(n), "{bytes:?}");
        }
    }

    #[test]
    fn modifier_parameters() {
        assert_eq!(key(b"\x1b[1;2A"), (KeyCode::Up, SHIFT, 6));
        assert_eq!(key(b"\x1b[1;3C"), (KeyCode::Right, ALT, 6));
        assert_eq!(key(b"\x1b[1;5D"), (KeyCode::Left, CTRL, 6));
        assert_eq!(
            key(b"\x1b[3;6~"),
            (
                KeyCode::Delete,
                Modifiers {
                    ctrl: true,
                    alt: false,
                    shift: true
                },
                6
            )
        );
    }

    #[test]
    fn partial_and_unknown_input() {
        assert_eq!(parse_key(b""), Parsed::Incomplete);
        assert_eq!(parse_key(b"\x1b"), Parsed::Incomplete);
        assert_eq!(parse_key(b"\x1b["), Parsed::Incomplete);
        assert_eq!(parse_key(b"\x1b[1;"), Parsed::Incomplete);
        assert_eq!(parse_key(b"\x1bO"), Parsed::Incomplete);
        assert_eq!(parse_key(&"é".as_bytes()[..1]), Parsed::Incomplete);
        assert_eq!(parse_key(b"\x1b[99~"), Parsed::Skip(5));
        assert_eq!(parse_key(b"\x1b[?"), Parsed::Skip(3));
        assert_eq!(parse_key(b"\x1bOz"), Parsed::Skip(3));
        assert_eq!(parse_key(b"\x1c"), Parsed::Skip(1));
        assert_eq!(parse_key(b"\xff"), Parsed::Skip(1));
        assert_eq!(parse_key(b"\xc3("), Parsed::Skip(1));
        assert_eq!(parse_key(b"\x1b\xff"), Parsed::Skip(2));
        assert_eq!(KeyEvent::from(KeyCode::Up).modifiers, Modifiers::NONE);
    }

    fn pipe() -> (RawFd, RawFd) {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `pipe` fills the array.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        fds.into()
    }

    fn write_fd(fd: RawFd, bytes: &[u8]) {
        // SAFETY: a valid buffer and descriptor.
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        assert_eq!(usize::try_from(n).unwrap(), bytes.len());
    }

    fn close(fd: RawFd) {
        // SAFETY: a descriptor this test opened.
        unsafe { libc::close(fd) };
    }

    #[test]
    fn the_reader_assembles_keys_and_tells_escape_from_sequences() {
        let (rd, wr) = pipe();
        let mut keys = Keys::on_fd(rd);
        assert!(!keys.poll(Duration::ZERO).unwrap());

        write_fd(wr, b"q\x1b[A\x1b[1;5");
        assert!(keys.poll(Duration::from_millis(100)).unwrap());
        assert_eq!(keys.read().unwrap().code, KeyCode::Char('q'));
        assert_eq!(keys.read().unwrap().code, KeyCode::Up);
        // The rest of a split sequence arrives later.
        assert!(!keys.poll(Duration::ZERO).unwrap());
        write_fd(wr, b"C");
        let event = keys.read().unwrap();
        assert_eq!((event.code, event.modifiers), (KeyCode::Right, CTRL));

        // A lone ESC is the Escape key once nothing follows.
        write_fd(wr, b"\x1b");
        assert!(keys.poll(Duration::from_millis(200)).unwrap());
        assert_eq!(keys.read().unwrap().code, KeyCode::Esc);

        // Unknown sequences are skipped, not returned.
        write_fd(wr, b"\x1b[99~x");
        assert!(keys.poll(Duration::from_millis(100)).unwrap());
        assert_eq!(keys.read().unwrap().code, KeyCode::Char('x'));

        // The end of input is an error, not a hang.
        close(wr);
        assert_eq!(keys.read().unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
        assert!(keys.poll(Duration::ZERO).is_err());
        close(rd);
        assert!(format!("{keys:?}").contains("Keys"));
        let _ = Keys::stdin();
    }

    #[test]
    fn an_unfinished_sequence_is_dropped_rather_than_wedging_the_reader() {
        let (rd, wr) = pipe();
        let mut keys = Keys::on_fd(rd);
        write_fd(wr, b"\x1b[1;");
        write_fd(wr, b"");
        let reader = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            write_fd(wr, b"z");
            wr
        });
        assert_eq!(keys.read().unwrap().code, KeyCode::Char('z'));
        close(reader.join().unwrap());
        close(rd);
    }
}
