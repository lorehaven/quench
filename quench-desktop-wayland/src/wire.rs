//! The Wayland wire format: messages as bytes, and nothing else.
//!
//! A message is a 32-bit object id, a 32-bit word with the total size in its upper half and the opcode in its
//! lower half, and then the arguments, each padded to 4 bytes. All integers are in the machine's byte order.
//! File descriptors are not in the bytes: they travel beside them (see `sys`).

use std::fmt;

/// Bytes in a message header.
pub const HEADER: usize = 8;
/// The largest message: the size field is 16 bits.
pub const MAX_MESSAGE: usize = 0xFFFF;

/// Why bytes are not a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    /// A size that is below the header, above the limit, or not a multiple of 4.
    BadSize(usize),
    /// An argument that runs past the end of the message.
    Short,
    /// A string that is not UTF-8, or has no terminating NUL.
    BadString,
    /// A message that would not fit in 65535 bytes.
    TooLong,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadSize(n) => write!(f, "a message of {n} bytes is not valid"),
            Self::Short => f.write_str("an argument runs past the end of its message"),
            Self::BadString => f.write_str("a string argument is not valid"),
            Self::TooLong => f.write_str("a message cannot be longer than 65535 bytes"),
        }
    }
}

impl std::error::Error for WireError {}

/// One message: the object it is for, what it asks or announces, and its arguments (undecoded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub object: u32,
    pub opcode: u16,
    pub args: Vec<u8>,
}

/// Builds the arguments of a request.
#[derive(Debug, Default, Clone)]
pub struct Args(Vec<u8>);

impl Args {
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    #[must_use]
    pub fn uint(mut self, value: u32) -> Self {
        self.0.extend_from_slice(&value.to_ne_bytes());
        self
    }

    #[must_use]
    pub fn int(mut self, value: i32) -> Self {
        self.0.extend_from_slice(&value.to_ne_bytes());
        self
    }

    /// A 24.8 fixed-point number.
    #[must_use]
    pub fn fixed(self, value: f64) -> Self {
        self.int((value * 256.0).round() as i32)
    }

    /// An object id (a `new_id` of a known interface is the same four bytes).
    #[must_use]
    pub fn object(self, id: u32) -> Self {
        self.uint(id)
    }

    /// A string: its length including the NUL, the bytes, the NUL, and padding to 4.
    #[must_use]
    pub fn string(mut self, text: &str) -> Self {
        let length = text.len() + 1;
        self.0.extend_from_slice(&(length as u32).to_ne_bytes());
        self.0.extend_from_slice(text.as_bytes());
        self.0.push(0);
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self
    }

    /// An array: its length, the bytes, and padding to 4.
    #[must_use]
    pub fn array(mut self, bytes: &[u8]) -> Self {
        self.0
            .extend_from_slice(&(bytes.len() as u32).to_ne_bytes());
        self.0.extend_from_slice(bytes);
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self
    }

    #[must_use]
    pub fn build(self) -> Vec<u8> {
        self.0
    }
}

/// A message as bytes.
///
/// # Errors
/// [`WireError::TooLong`] when it would not fit the size field.
pub fn encode(object: u32, opcode: u16, args: &[u8]) -> Result<Vec<u8>, WireError> {
    debug_assert!(
        args.len().is_multiple_of(4),
        "arguments are padded to 4 bytes"
    );
    let size = HEADER + args.len();
    if size > MAX_MESSAGE {
        return Err(WireError::TooLong);
    }
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&object.to_ne_bytes());
    out.extend_from_slice(&(((size as u32) << 16) | u32::from(opcode)).to_ne_bytes());
    out.extend_from_slice(args);
    Ok(out)
}

/// Cuts a stream of bytes into messages, whatever way it arrives in pieces.
#[derive(Debug, Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}

impl Decoder {
    #[must_use]
    pub const fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole message, or `None` until the rest of it has been fed.
    ///
    /// # Errors
    /// [`WireError::BadSize`] for a header that cannot be a message; the stream is then not trustworthy.
    pub fn next_message(&mut self) -> Result<Option<Message>, WireError> {
        if self.buffer.len() < HEADER {
            return Ok(None);
        }
        let word = u32::from_ne_bytes([
            self.buffer[4],
            self.buffer[5],
            self.buffer[6],
            self.buffer[7],
        ]);
        let size = (word >> 16) as usize;
        if size < HEADER || !size.is_multiple_of(4) {
            return Err(WireError::BadSize(size));
        }
        if self.buffer.len() < size {
            return Ok(None);
        }
        let object = u32::from_ne_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]);
        let args = self.buffer[HEADER..size].to_vec();
        self.buffer.drain(..size);
        Ok(Some(Message {
            object,
            opcode: (word & 0xFFFF) as u16,
            args,
        }))
    }
}

/// Reads the arguments of an event.
#[derive(Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], WireError> {
        let end = self.at.checked_add(n).ok_or(WireError::Short)?;
        let slice = self.data.get(self.at..end).ok_or(WireError::Short)?;
        self.at = end;
        Ok(slice)
    }

    /// # Errors
    /// [`WireError::Short`] when the message has no more arguments.
    pub fn uint(&mut self) -> Result<u32, WireError> {
        let b = self.take(4)?;
        Ok(u32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// # Errors
    /// As [`uint`](Self::uint).
    pub fn int(&mut self) -> Result<i32, WireError> {
        self.uint().map(|v| v as i32)
    }

    /// # Errors
    /// As [`uint`](Self::uint).
    pub fn fixed(&mut self) -> Result<f64, WireError> {
        self.int().map(|v| f64::from(v) / 256.0)
    }

    /// An object id.
    ///
    /// # Errors
    /// As [`uint`](Self::uint).
    pub fn object(&mut self) -> Result<u32, WireError> {
        self.uint()
    }

    /// A string; a null string (length 0) reads as empty.
    ///
    /// # Errors
    /// [`WireError::BadString`] without the terminating NUL or when not UTF-8; [`WireError::Short`].
    pub fn string(&mut self) -> Result<String, WireError> {
        let length = self.uint()? as usize;
        if length == 0 {
            return Ok(String::new());
        }
        let padded = length.next_multiple_of(4);
        let bytes = self.take(padded)?;
        let (text, nul) = bytes[..length].split_at(length - 1);
        if nul != [0] {
            return Err(WireError::BadString);
        }
        String::from_utf8(text.to_vec()).map_err(|_| WireError::BadString)
    }

    /// An array's bytes.
    ///
    /// # Errors
    /// [`WireError::Short`].
    pub fn array(&mut self) -> Result<&'a [u8], WireError> {
        let length = self.uint()? as usize;
        let padded = length.next_multiple_of(4);
        Ok(&self.take(padded)?[..length])
    }

    /// Whether every argument has been read.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.at == self.data.len()
    }
}
