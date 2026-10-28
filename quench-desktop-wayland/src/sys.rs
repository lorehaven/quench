//! What Rust's standard library cannot do for a Wayland client: pass file descriptors over a socket, make and
//! map shared memory.
//!
//! Every `unsafe` of the crate is in this file; the rest is safe code over these types.
//!
//! The contract the safe types keep:
//!
//! - **Descriptors have one owner.** A descriptor that arrives with a message becomes an [`OwnedFd`] at once
//!   (and is closed if nobody takes it); one that is sent is only borrowed for the call.
//! - **A mapping lives as long as its value.** [`Mapping`] unmaps on drop, and its slices borrow it, so they
//!   cannot outlive the memory. The memory is *shared*: the compositor reads it too. Rust cannot see that, so
//!   the caller must not write a buffer the compositor may be reading (a window keeps two and writes the one
//!   it has been told is released) and must treat what it reads back as data that may change.
//! - **Control data is sized and aligned for what it holds** (a `u64` buffer for `cmsghdr`), and a message
//!   whose descriptors did not all fit is an error, not a silent loss.
#![allow(unsafe_code)] // the whole point of this file

use std::io::{self, Write};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// How many descriptors one `recvmsg` can bring (a compositor sends one at a time: a keymap).
const MAX_FDS: usize = 8;

/// A connection to the compositor.
#[derive(Debug)]
pub struct Socket {
    stream: UnixStream,
}

impl Socket {
    /// Connects to `$WAYLAND_DISPLAY` (a name under `$XDG_RUNTIME_DIR`, or an absolute path).
    ///
    /// # Errors
    /// When the variables are missing or nothing listens there.
    pub fn connect_display() -> io::Result<Self> {
        let name = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
        let path = if Path::new(&name).is_absolute() {
            std::path::PathBuf::from(name)
        } else {
            let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR is not set")
            })?;
            Path::new(&runtime).join(name)
        };
        Self::connect_path(&path)
    }

    /// Connects to the socket at `path`.
    ///
    /// # Errors
    /// When nothing listens there.
    pub fn connect_path(path: &Path) -> io::Result<Self> {
        UnixStream::connect(path).map(Self::from_stream)
    }

    #[must_use]
    pub const fn from_stream(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// How long `recv` waits for something to arrive (`None`: for ever).
    ///
    /// # Errors
    /// When the socket refuses the option.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.stream.set_read_timeout(timeout)
    }

    /// Sends `data` and, with its first byte, `fds` (the other side receives them with that message).
    ///
    /// # Errors
    /// The socket's error (a closed connection, for one).
    pub fn send(&self, data: &[u8], fds: &[BorrowedFd<'_>]) -> io::Result<()> {
        let mut iov = libc::iovec {
            iov_base: data.as_ptr().cast_mut().cast(),
            iov_len: data.len(),
        };
        // SAFETY: an all-zero `msghdr` is valid (null pointers, zero lengths).
        let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
        message.msg_iov = &raw mut iov;
        message.msg_iovlen = 1;
        let fd_bytes = std::mem::size_of_val(fds);
        // `u64` for the alignment `cmsghdr` needs; it outlives the `sendmsg` call below.
        let mut control: Vec<u64> = Vec::new();
        if !fds.is_empty() {
            // SAFETY: `CMSG_SPACE`/`CMSG_LEN` only compute sizes.
            let space = unsafe { libc::CMSG_SPACE(fd_bytes as u32) } as usize;
            control = vec![0; space.div_ceil(8)];
            message.msg_control = control.as_mut_ptr().cast();
            message.msg_controllen = space as _;
            // SAFETY: `msg_control` points to `space` zeroed, aligned bytes, so the first header is inside
            // it and its data area has room for `fd_bytes`; the descriptors are plain integers.
            unsafe {
                let header = libc::CMSG_FIRSTHDR(&raw const message);
                (*header).cmsg_level = libc::SOL_SOCKET;
                (*header).cmsg_type = libc::SCM_RIGHTS;
                (*header).cmsg_len = libc::CMSG_LEN(fd_bytes as u32) as _;
                let target = libc::CMSG_DATA(header).cast::<RawFd>();
                for (i, fd) in fds.iter().enumerate() {
                    target.add(i).write_unaligned(fd.as_raw_fd());
                }
            }
        }
        let sent = loop {
            // SAFETY: `message` and what it points to (`iov`, `data`, `control`) are alive for the call.
            let n = unsafe {
                libc::sendmsg(
                    self.stream.as_raw_fd(),
                    &raw const message,
                    libc::MSG_NOSIGNAL,
                )
            };
            if n >= 0 {
                break n as usize;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        };
        drop(control);
        if sent < data.len() {
            (&self.stream).write_all(&data[sent..])?;
        }
        Ok(())
    }

    /// Reads what has arrived into `buf`: how many bytes, and the descriptors that came with them. Zero bytes
    /// is the end of the connection.
    ///
    /// # Errors
    /// The socket's error (`WouldBlock` after the read timeout), or descriptors that did not fit.
    pub fn recv(&self, buf: &mut [u8]) -> io::Result<(usize, Vec<OwnedFd>)> {
        let mut iov = libc::iovec {
            iov_base: buf.as_mut_ptr().cast(),
            iov_len: buf.len(),
        };
        // SAFETY: as in `send`.
        let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
        message.msg_iov = &raw mut iov;
        message.msg_iovlen = 1;
        // SAFETY: only computes a size.
        let space =
            unsafe { libc::CMSG_SPACE((MAX_FDS * std::mem::size_of::<RawFd>()) as u32) } as usize;
        let mut control = vec![0u64; space.div_ceil(8)];
        message.msg_control = control.as_mut_ptr().cast();
        message.msg_controllen = space as _;
        let received = loop {
            // SAFETY: `message`, `iov`, `buf` and `control` are alive and sized as declared.
            let n = unsafe {
                libc::recvmsg(
                    self.stream.as_raw_fd(),
                    &raw mut message,
                    libc::MSG_CMSG_CLOEXEC,
                )
            };
            if n >= 0 {
                break n as usize;
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        };
        let mut fds = Vec::new();
        // SAFETY: after a successful `recvmsg` the control buffer holds well-formed headers up to
        // `msg_controllen`; each `SCM_RIGHTS` payload is whole descriptors the kernel has just made ours.
        unsafe {
            let mut header = libc::CMSG_FIRSTHDR(&raw const message);
            while !header.is_null() {
                if (*header).cmsg_level == libc::SOL_SOCKET
                    && (*header).cmsg_type == libc::SCM_RIGHTS
                {
                    let payload = (*header).cmsg_len as usize - libc::CMSG_LEN(0) as usize;
                    let data = libc::CMSG_DATA(header).cast::<RawFd>();
                    for i in 0..payload / std::mem::size_of::<RawFd>() {
                        fds.push(OwnedFd::from_raw_fd(data.add(i).read_unaligned()));
                    }
                }
                header = libc::CMSG_NXTHDR(&raw const message, header);
            }
        }
        if message.msg_flags & libc::MSG_CTRUNC != 0 {
            return Err(io::Error::other(
                "descriptors were lost: more than fit in one message",
            ));
        }
        Ok((received, fds))
    }
}

/// An anonymous file of `len` bytes: memory that can be mapped and shared by descriptor.
///
/// # Errors
/// When the kernel refuses it.
pub fn memfd(len: usize) -> io::Result<OwnedFd> {
    // SAFETY: `memfd_create` takes a NUL-terminated name and flags, and returns a descriptor or -1.
    let raw = unsafe { libc::memfd_create(c"quench-wayland".as_ptr(), libc::MFD_CLOEXEC) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a fresh descriptor nobody else owns.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let length =
        libc::off_t::try_from(len).map_err(|_| io::Error::other("the buffer is too large"))?;
    // SAFETY: the descriptor is valid and `length` is a non-negative size.
    if unsafe { libc::ftruncate(fd.as_raw_fd(), length) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}

/// A shared mapping of a file, unmapped when dropped.
#[derive(Debug)]
pub struct Mapping {
    ptr: *mut u8,
    len: usize,
    writable: bool,
}

impl Mapping {
    /// Maps `len` bytes of `fd`, readable and, when `writable`, writable.
    ///
    /// # Errors
    /// When `len` is 0 or the kernel refuses (the file is shorter, for one).
    pub fn new(fd: BorrowedFd<'_>, len: usize, writable: bool) -> io::Result<Self> {
        if len == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "cannot map 0 bytes",
            ));
        }
        let protection = libc::PROT_READ | if writable { libc::PROT_WRITE } else { 0 };
        // SAFETY: a null address lets the kernel choose; `fd` is valid for the call; the result is checked.
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                protection,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            ptr: ptr.cast(),
            len,
            writable,
        })
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The bytes. The compositor may be writing them too (it never does for a buffer we own), so a value read
    /// from a mapping of someone else's memory is data, not a promise.
    #[must_use]
    pub const fn as_slice(&self) -> &[u8] {
        // SAFETY: `ptr` points to `len` mapped, readable bytes until `self` is dropped, which the borrow
        // prevents while this slice lives.
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    /// The bytes, for writing.
    ///
    /// # Panics
    /// When the mapping was made read-only (writing would fault).
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        assert!(self.writable, "this mapping is read-only");
        // SAFETY: as for `as_slice`, the mapping is writable (checked), and `&mut self` makes this the only
        // slice on our side.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: `ptr` and `len` are what `mmap` returned and was asked for, and it is unmapped once.
        unsafe { libc::munmap(self.ptr.cast(), self.len) };
    }
}
