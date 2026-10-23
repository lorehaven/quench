//! Raw terminal mode on Unix, through `libc` termios.
//!
//! This is the only place in the workspace's CLI crate that needs `unsafe`:
//! each call is a plain libc function on a file descriptor and a struct the
//! caller owns.

#![allow(unsafe_code)]

use std::io;

/// Puts a terminal in raw-ish mode (no echo, no line buffering, no signals
/// from the keyboard, no CR translation) and restores it on drop.
#[derive(Debug)]
pub struct RawMode {
    fd: i32,
    original: libc::termios,
}

impl RawMode {
    /// Enables raw mode on `fd`. `None` when `fd` is not a terminal.
    ///
    /// # Errors
    ///
    /// The OS error if the terminal's attributes cannot be read or set.
    pub fn enable(fd: i32) -> io::Result<Option<Self>> {
        // SAFETY: `isatty` only inspects the descriptor.
        if unsafe { libc::isatty(fd) } != 1 {
            return Ok(None);
        }
        let mut original = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `tcgetattr` fills the struct it is given on success.
        if unsafe { libc::tcgetattr(fd, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: initialised by the successful `tcgetattr` above.
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG | libc::IEXTEN);
        raw.c_iflag &= !(libc::IXON | libc::ICRNL | libc::INLCR | libc::ISTRIP);
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        // SAFETY: `raw` is a valid termios derived from the one the kernel gave us.
        if unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &raw const raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(Self { fd, original }))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // SAFETY: restores the attributes saved in `enable`; nothing to do on failure.
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSAFLUSH, &raw const self.original);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::RawFd;

    /// A pseudo-terminal pair: (master, slave).
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
        assert_eq!(rc, 0, "openpty failed: {}", io::Error::last_os_error());
        (master, slave)
    }

    fn local_flags(fd: RawFd) -> libc::tcflag_t {
        let mut attrs = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: filled by `tcgetattr`.
        assert_eq!(unsafe { libc::tcgetattr(fd, attrs.as_mut_ptr()) }, 0);
        // SAFETY: initialised above.
        unsafe { attrs.assume_init() }.c_lflag
    }

    fn close(fd: RawFd) {
        // SAFETY: a descriptor this test opened.
        unsafe { libc::close(fd) };
    }

    #[test]
    fn raw_mode_is_entered_and_restored() {
        let (master, slave) = pty();
        let before = local_flags(slave);
        assert_ne!(before & libc::ECHO, 0, "a fresh pty echoes");
        {
            let raw = RawMode::enable(slave)
                .unwrap()
                .expect("a pty is a terminal");
            let during = local_flags(slave);
            assert_eq!(during & (libc::ECHO | libc::ICANON | libc::ISIG), 0);
            assert!(format!("{raw:?}").contains("RawMode"));
        }
        assert_eq!(
            local_flags(slave),
            before,
            "dropping restores the attributes"
        );
        close(master);
        close(slave);
    }

    #[test]
    fn non_terminals_are_not_touched() {
        let mut fds = [0 as RawFd; 2];
        // SAFETY: `pipe` fills the two-element array.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        assert!(RawMode::enable(fds[0]).unwrap().is_none());
        close(fds[0]);
        close(fds[1]);
        assert!(RawMode::enable(-1).unwrap().is_none());
    }
}
