//! A connection to a D-Bus bus (or to a peer): authentication, serials, sending and receiving.

use std::io::{self, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::{SocketAddr, UnixStream};
use std::sync::{Arc, Mutex};

use crate::message::{Kind, Message};
use crate::value::Value;

/// The address of a bus: `unix:path=...` or `unix:abstract=...` (more `,key=value` pairs and `;`-separated
/// alternatives are ignored past the first usable one).
///
/// # Errors
/// An address that is not a Unix socket, or one that cannot be connected to.
pub fn connect(address: &str) -> io::Result<UnixStream> {
    let mut last = io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("no usable address in {address:?}"),
    );
    for candidate in address.split(';') {
        let Some(rest) = candidate.strip_prefix("unix:") else {
            continue;
        };
        let attempt = rest.split(',').find_map(|pair| {
            let (key, value) = pair.split_once('=')?;
            let value = unescape(value);
            match key {
                "path" => Some(UnixStream::connect(&value)),
                "abstract" => Some(
                    SocketAddr::from_abstract_name(value.as_bytes())
                        .and_then(|a| UnixStream::connect_addr(&a)),
                ),
                _ => None,
            }
        });
        match attempt {
            Some(Ok(stream)) => return Ok(stream),
            Some(Err(e)) => last = e,
            None => {}
        }
    }
    Err(last)
}

/// `%xx` escapes of a D-Bus address value.
fn unescape(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = value.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The user id, as the EXTERNAL mechanism says it: read from `/proc/self`, which belongs to this user.
fn uid() -> io::Result<u32> {
    Ok(std::fs::metadata("/proc/self")?.uid())
}

/// One line of the authentication dialogue, read a byte at a time: what follows the line (the first message)
/// must stay in the stream, not in a buffer that is dropped with the reader.
fn read_line(stream: &mut impl Read) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the peer closed during authentication",
            ));
        }
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
        if line.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "an authentication line that long is not one",
            ));
        }
    }
    Ok(String::from_utf8_lossy(&line).trim_end().to_string())
}

/// Authenticates as a client with EXTERNAL (the credentials are the socket's).
///
/// # Errors
/// The stream's, or a refusal.
pub fn authenticate_client(stream: &mut UnixStream) -> io::Result<()> {
    let hex: String = uid()?
        .to_string()
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect();
    stream.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
    let answer = read_line(stream)?;
    if !answer.starts_with("OK") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("authentication refused: {answer}"),
        ));
    }
    stream.write_all(b"BEGIN\r\n")
}

/// The other end of [`authenticate_client`], for a peer that accepts a direct connection.
///
/// # Errors
/// The stream's, or a client that does not ask for EXTERNAL.
pub fn authenticate_server(stream: &mut UnixStream) -> io::Result<()> {
    let mut first = [0u8; 1];
    stream.read_exact(&mut first)?;
    let auth = read_line(stream)?;
    if !auth.starts_with("AUTH EXTERNAL") {
        stream.write_all(b"REJECTED EXTERNAL\r\n")?;
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "only EXTERNAL authentication is accepted",
        ));
    }
    stream.write_all(b"OK 0123456789abcdef0123456789abcdef\r\n")?;
    let begin = read_line(stream)?;
    if begin != "BEGIN" {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "expected BEGIN"));
    }
    Ok(())
}

/// A connection. Cloning the writing half is cheap: replies and signals can be sent from any thread.
pub struct Connection {
    reader: UnixStream,
    writer: Arc<Mutex<UnixStream>>,
    serial: Arc<Mutex<u32>>,
    /// The name the bus gave this connection (`:1.42`), or what the peer was told to be.
    pub name: String,
}

/// What is needed to send from another thread.
#[derive(Clone)]
pub struct Sender {
    writer: Arc<Mutex<UnixStream>>,
    serial: Arc<Mutex<u32>>,
}

impl Sender {
    /// Sends `message` with the next serial number; returns the serial.
    ///
    /// # Errors
    /// The stream's.
    pub fn send(&self, message: &mut Message) -> io::Result<u32> {
        let mut serial = self
            .serial
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *serial += 1;
        message.serial = *serial;
        let bytes = message.encode();
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        writer.write_all(&bytes)?;
        Ok(message.serial)
    }
}

impl Connection {
    /// A connection over an authenticated stream.
    ///
    /// # Errors
    /// The stream's.
    pub fn new(stream: UnixStream, name: &str) -> io::Result<Self> {
        Ok(Self {
            writer: Arc::new(Mutex::new(stream.try_clone()?)),
            reader: stream,
            serial: Arc::new(Mutex::new(0)),
            name: name.to_string(),
        })
    }

    /// Connects to a bus, authenticates and says hello: `name` is the unique name the bus assigned.
    ///
    /// # Errors
    /// The connection's, or a bus that does not answer as one.
    pub fn bus(address: &str) -> io::Result<Self> {
        let mut stream = connect(address)?;
        authenticate_client(&mut stream)?;
        let mut connection = Self::new(stream, "")?;
        let reply = connection.call(Message::call(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "Hello",
            Vec::new(),
        ))?;
        connection.name = reply
            .body
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Hello gave no name"))?
            .to_string();
        Ok(connection)
    }

    #[must_use]
    pub fn sender(&self) -> Sender {
        Sender {
            writer: Arc::clone(&self.writer),
            serial: Arc::clone(&self.serial),
        }
    }

    /// Sends a message.
    ///
    /// # Errors
    /// The stream's.
    pub fn send(&self, mut message: Message) -> io::Result<u32> {
        self.sender().send(&mut message)
    }

    /// The next message from the other end.
    ///
    /// # Errors
    /// The stream's (a timeout is `WouldBlock` or `TimedOut`).
    pub fn recv(&mut self) -> io::Result<Message> {
        Message::read(&mut self.reader)
    }

    /// Sets how long [`recv`](Self::recv) waits (`None`: for ever).
    ///
    /// # Errors
    /// The stream's.
    pub fn set_timeout(&self, timeout: Option<std::time::Duration>) -> io::Result<()> {
        self.reader.set_read_timeout(timeout)
    }

    /// Calls a method and waits for its answer; messages that arrive meanwhile and are not the answer are
    /// dropped.
    ///
    /// # Errors
    /// The stream's, or the error the callee returned.
    pub fn call(&mut self, message: Message) -> io::Result<Message> {
        let serial = self.send(message)?;
        loop {
            let m = self.recv()?;
            if m.reply_serial == Some(serial) {
                return match m.kind {
                    Kind::Error => Err(io::Error::other(format!(
                        "{}: {}",
                        m.error_name.unwrap_or_default(),
                        m.body.first().and_then(Value::as_str).unwrap_or("")
                    ))),
                    _ => Ok(m),
                };
            }
        }
    }
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Sender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}
