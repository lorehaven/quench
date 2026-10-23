//! WebSocket (RFC 6455): the opening handshake, frame codec and a message-level
//! connection that handles fragmentation, ping/pong and the closing handshake.
//!
//! No extensions are negotiated (no permessage-deflate), so any frame with a
//! reserved bit set is a protocol error.

use crate::header::HeaderMap;
use crate::header::HeaderValue;
use crate::io::{Io, Upgraded};
use crate::message::Response;
use crate::method::Method;
use crate::status::StatusCode;
use quench_core::base64;
use std::fmt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// The GUID the handshake appends to the client key (RFC 6455 section 1.3).
pub const HANDSHAKE_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// `Sec-WebSocket-Accept` for a given `Sec-WebSocket-Key`.
#[must_use]
pub fn accept_key(client_key: &str) -> String {
    let mut input = client_key.trim().to_string();
    input.push_str(HANDSHAKE_GUID);
    base64::STANDARD.encode(quench_crypto::hash::sha1_legacy(input))
}

/// A fresh random `Sec-WebSocket-Key` for a client handshake.
#[must_use]
pub fn generate_key() -> String {
    base64::STANDARD.encode(quench_core::random::bytes::<16>())
}

/// Why a request is not a valid WebSocket upgrade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeError {
    NotGet,
    MissingUpgrade,
    MissingConnection,
    MissingKey,
    /// The key is not 16 bytes of base64.
    BadKey,
    UnsupportedVersion,
}

impl fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotGet => "WebSocket upgrades must be GET requests",
            Self::MissingUpgrade => "missing `Upgrade: websocket`",
            Self::MissingConnection => "missing `Connection: Upgrade`",
            Self::MissingKey => "missing Sec-WebSocket-Key",
            Self::BadKey => "Sec-WebSocket-Key is not a 16-byte base64 value",
            Self::UnsupportedVersion => "only Sec-WebSocket-Version 13 is supported",
        })
    }
}

impl std::error::Error for HandshakeError {}

/// Checks the method and headers of a WebSocket upgrade request and returns its key.
///
/// # Errors
///
/// The first [`HandshakeError`] found.
pub fn validate_upgrade(method: &Method, headers: &HeaderMap) -> Result<String, HandshakeError> {
    if *method != Method::GET {
        return Err(HandshakeError::NotGet);
    }
    if !crate::h1::parse::has_token(headers, "upgrade", "websocket") {
        return Err(HandshakeError::MissingUpgrade);
    }
    if !crate::h1::parse::has_token(headers, "connection", "upgrade") {
        return Err(HandshakeError::MissingConnection);
    }
    if headers
        .get("sec-websocket-version")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        != Some("13")
    {
        return Err(HandshakeError::UnsupportedVersion);
    }
    let key = headers
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(HandshakeError::MissingKey)?
        .trim();
    match base64::STANDARD.decode(key) {
        Ok(raw) if raw.len() == 16 => Ok(key.to_string()),
        _ => Err(HandshakeError::BadKey),
    }
}

/// Limits for a WebSocket connection.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    /// Largest accepted message, reassembled from fragments.
    pub max_message_size: usize,
    /// Largest accepted single frame payload.
    pub max_frame_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_message_size: 16 * 1024 * 1024,
            max_frame_size: 16 * 1024 * 1024,
        }
    }
}

/// Answers a valid upgrade request with `101 Switching Protocols` and runs
/// `handler` on the resulting connection.
///
/// # Errors
///
/// A [`HandshakeError`] when the request is not a WebSocket upgrade; answer it with `400`.
pub fn accept<F, Fut>(
    method: &Method,
    headers: &HeaderMap,
    config: Config,
    handler: F,
) -> Result<Response, HandshakeError>
where
    F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let key = validate_upgrade(method, headers)?;
    Ok(switch(&key, config, handler))
}

/// The `101 Switching Protocols` response for an already validated `key`
/// (see [`validate_upgrade`]), running `handler` once the connection switched.
pub fn switch<F, Fut>(key: &str, config: Config, handler: F) -> Response
where
    F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    Response::new(StatusCode::SWITCHING_PROTOCOLS)
        .header("upgrade", "websocket")
        .header("connection", "Upgrade")
        .header("sec-websocket-accept", &accept_key(key))
        .on_upgrade(move |io| handler(WebSocket::new(io, Role::Server, config)))
}

/// A frame's meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Continuation,
    Text,
    Binary,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0x0 => Self::Continuation,
            0x1 => Self::Text,
            0x2 => Self::Binary,
            0x8 => Self::Close,
            0x9 => Self::Ping,
            0xA => Self::Pong,
            _ => return None,
        })
    }

    const fn to_u8(self) -> u8 {
        match self {
            Self::Continuation => 0x0,
            Self::Text => 0x1,
            Self::Binary => 0x2,
            Self::Close => 0x8,
            Self::Ping => 0x9,
            Self::Pong => 0xA,
        }
    }

    #[must_use]
    pub const fn is_control(self) -> bool {
        self.to_u8() >= 0x8
    }
}

/// One decoded frame, payload already unmasked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub fin: bool,
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

/// A violation of the framing rules, with the close code to answer it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    ReservedBits,
    UnknownOpcode,
    FragmentedControlFrame,
    ControlFrameTooLong,
    /// Client frames must be masked, server frames must not be.
    BadMasking,
    UnexpectedContinuation,
    /// A new data frame arrived while a fragmented message was open.
    InterruptedMessage,
    NonMinimalLength,
    InvalidClosePayload,
    InvalidCloseCode,
    InvalidUtf8,
    TooBig,
}

impl ProtocolError {
    /// The close status code to send for this error.
    #[must_use]
    pub const fn close_code(self) -> u16 {
        match self {
            Self::InvalidUtf8 | Self::InvalidClosePayload => 1007,
            Self::TooBig => 1009,
            _ => 1002,
        }
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ReservedBits => "reserved bits set without a negotiated extension",
            Self::UnknownOpcode => "unknown opcode",
            Self::FragmentedControlFrame => "fragmented control frame",
            Self::ControlFrameTooLong => "control frame payload over 125 bytes",
            Self::BadMasking => "wrong masking for this direction",
            Self::UnexpectedContinuation => "continuation frame without a message to continue",
            Self::InterruptedMessage => "data frame inside a fragmented message",
            Self::NonMinimalLength => "payload length not minimally encoded",
            Self::InvalidClosePayload => "malformed close payload",
            Self::InvalidCloseCode => "invalid close status code",
            Self::InvalidUtf8 => "text is not valid UTF-8",
            Self::TooBig => "message too big",
        })
    }
}

impl std::error::Error for ProtocolError {}

/// Appends one frame. `mask` is `Some` for client-to-server frames.
pub fn encode_frame(
    out: &mut Vec<u8>,
    fin: bool,
    opcode: Opcode,
    payload: &[u8],
    mask: Option<[u8; 4]>,
) {
    out.push(u8::from(fin) << 7 | opcode.to_u8());
    let mask_bit = if mask.is_some() { 0x80 } else { 0 };
    let len = payload.len();
    if let Ok(short) = u8::try_from(len)
        && short < 126
    {
        out.push(mask_bit | short);
    } else if let Ok(len) = u16::try_from(len) {
        out.push(mask_bit | 0x7e);
        out.extend_from_slice(&len.to_be_bytes());
    } else {
        out.push(mask_bit | 0x7f);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    match mask {
        Some(key) => {
            out.extend_from_slice(&key);
            out.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i % 4]));
        }
        None => out.extend_from_slice(payload),
    }
}

/// Decodes one frame from the front of `buf`.
///
/// Returns the frame and how many bytes it used, or `None` when `buf` holds
/// only part of a frame. `expect_masked` is true on a server (clients must mask).
///
/// # Errors
///
/// A [`ProtocolError`] for any framing violation, detected as early as the header allows.
pub fn decode_frame(
    buf: &[u8],
    expect_masked: bool,
    max_payload: usize,
) -> Result<Option<(Frame, usize)>, ProtocolError> {
    let [b0, b1, ..] = *buf else { return Ok(None) };
    if b0 & 0x70 != 0 {
        return Err(ProtocolError::ReservedBits);
    }
    let fin = b0 & 0x80 != 0;
    let opcode = Opcode::from_u8(b0 & 0x0f).ok_or(ProtocolError::UnknownOpcode)?;
    let masked = b1 & 0x80 != 0;
    if masked != expect_masked {
        return Err(ProtocolError::BadMasking);
    }
    let (len, mut pos) = match b1 & 0x7f {
        126 => {
            let Some(bytes) = buf.get(2..4) else {
                return Ok(None);
            };
            let len = u64::from(u16::from_be_bytes([bytes[0], bytes[1]]));
            if len < 126 {
                return Err(ProtocolError::NonMinimalLength);
            }
            (len, 4)
        }
        127 => {
            let Some(bytes) = buf.get(2..10) else {
                return Ok(None);
            };
            let len = u64::from_be_bytes(bytes.try_into().unwrap_or([0; 8]));
            if len >> 63 != 0 || len < 65_536 {
                return Err(ProtocolError::NonMinimalLength);
            }
            (len, 10)
        }
        short => (u64::from(short), 2),
    };
    if opcode.is_control() {
        if !fin {
            return Err(ProtocolError::FragmentedControlFrame);
        }
        if len > 125 {
            return Err(ProtocolError::ControlFrameTooLong);
        }
    }
    let len = usize::try_from(len).map_err(|_| ProtocolError::TooBig)?;
    if len > max_payload {
        return Err(ProtocolError::TooBig);
    }
    let key = if masked {
        let Some(bytes) = buf.get(pos..pos + 4) else {
            return Ok(None);
        };
        pos += 4;
        Some([bytes[0], bytes[1], bytes[2], bytes[3]])
    } else {
        None
    };
    let Some(raw) = buf.get(pos..pos + len) else {
        return Ok(None);
    };
    let payload = key.map_or_else(
        || raw.to_vec(),
        |key| {
            raw.iter()
                .enumerate()
                .map(|(i, b)| b ^ key[i % 4])
                .collect()
        },
    );
    Ok(Some((
        Frame {
            fin,
            opcode,
            payload,
        },
        pos + len,
    )))
}

/// A close frame's status and reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseFrame {
    pub code: u16,
    pub reason: String,
}

impl CloseFrame {
    fn parse(payload: &[u8]) -> Result<Option<Self>, ProtocolError> {
        match payload {
            [] => Ok(None),
            [_] => Err(ProtocolError::InvalidClosePayload),
            [hi, lo, reason @ ..] => {
                let code = u16::from_be_bytes([*hi, *lo]);
                if !matches!(code, 1000..=1003 | 1007..=1011 | 3000..=4999) {
                    return Err(ProtocolError::InvalidCloseCode);
                }
                let reason = std::str::from_utf8(reason).map_err(|_| ProtocolError::InvalidUtf8)?;
                Ok(Some(Self {
                    code,
                    reason: reason.to_string(),
                }))
            }
        }
    }

    fn payload(&self) -> Vec<u8> {
        let mut out = self.code.to_be_bytes().to_vec();
        out.extend_from_slice(self.reason.as_bytes());
        out
    }
}

/// A complete message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Text(String),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong(Vec<u8>),
    Close(Option<CloseFrame>),
}

/// Which end of the connection this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Server,
    Client,
}

/// Why a WebSocket operation failed.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Protocol(ProtocolError),
    /// The connection is closed; nothing more can be sent.
    Closed,
    /// A control message payload over 125 bytes.
    ControlTooLong,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(err) => write!(f, "websocket i/o error: {err}"),
            Self::Protocol(err) => write!(f, "websocket protocol error: {err}"),
            Self::Closed => f.write_str("websocket is closed"),
            Self::ControlTooLong => f.write_str("control payload over 125 bytes"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Protocol(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// A message-level WebSocket connection.
pub struct WebSocket<I> {
    io: I,
    role: Role,
    config: Config,
    inbox: Vec<u8>,
    /// Payload and kind of a fragmented message in progress.
    partial: Option<(Opcode, Vec<u8>)>,
    close_sent: bool,
    closed: bool,
}

impl<I> fmt::Debug for WebSocket<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocket")
            .field("role", &self.role)
            .field("closed", &self.closed)
            .finish_non_exhaustive()
    }
}

impl<I: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> WebSocket<I> {
    pub const fn new(io: I, role: Role, config: Config) -> Self {
        Self {
            io,
            role,
            config,
            inbox: Vec::new(),
            partial: None,
            close_sent: false,
            closed: false,
        }
    }

    #[must_use]
    pub const fn is_closed(&self) -> bool {
        self.closed
    }

    async fn write_frame(&mut self, opcode: Opcode, payload: &[u8]) -> Result<(), Error> {
        let mask = (self.role == Role::Client).then(quench_core::random::bytes::<4>);
        let mut out = Vec::with_capacity(payload.len() + 14);
        encode_frame(&mut out, true, opcode, payload, mask);
        self.io.write_all(&out).await?;
        self.io.flush().await?;
        Ok(())
    }

    /// Sends a message. Sending after a close was sent fails with [`Error::Closed`].
    ///
    /// # Errors
    ///
    /// [`Error::Closed`], [`Error::ControlTooLong`] or an I/O error.
    pub async fn send(&mut self, message: Message) -> Result<(), Error> {
        if self.close_sent {
            return Err(Error::Closed);
        }
        match message {
            Message::Text(text) => self.write_frame(Opcode::Text, text.as_bytes()).await,
            Message::Binary(data) => self.write_frame(Opcode::Binary, &data).await,
            Message::Ping(data) | Message::Pong(data) if data.len() > 125 => {
                Err(Error::ControlTooLong)
            }
            Message::Ping(data) => self.write_frame(Opcode::Ping, &data).await,
            Message::Pong(data) => self.write_frame(Opcode::Pong, &data).await,
            Message::Close(frame) => self.send_close(frame).await,
        }
    }

    async fn send_close(&mut self, frame: Option<CloseFrame>) -> Result<(), Error> {
        let payload = frame.as_ref().map(CloseFrame::payload).unwrap_or_default();
        if payload.len() > 125 {
            return Err(Error::ControlTooLong);
        }
        self.close_sent = true;
        self.write_frame(Opcode::Close, &payload).await
    }

    /// Starts the closing handshake with `code` and `reason` (truncated to fit a control frame).
    ///
    /// # Errors
    ///
    /// An I/O error while writing.
    pub async fn close(&mut self, code: u16, reason: &str) -> Result<(), Error> {
        if self.close_sent {
            return Ok(());
        }
        let mut end = reason.len().min(123);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        self.send_close(Some(CloseFrame {
            code,
            reason: reason[..end].to_string(),
        }))
        .await
    }

    /// The next message. Pings are answered automatically and not returned. A
    /// peer `Close` is echoed and returned once; afterwards this yields `Ok(None)`.
    ///
    /// # Errors
    ///
    /// A protocol violation (after sending the matching close code) or an I/O error.
    pub async fn recv(&mut self) -> Result<Option<Message>, Error> {
        loop {
            if self.closed {
                return Ok(None);
            }
            let decoded = decode_frame(
                &self.inbox,
                self.role == Role::Server,
                self.config.max_frame_size,
            );
            match decoded {
                Err(err) => return Err(self.fail(err).await),
                Ok(Some((frame, used))) => {
                    self.inbox.drain(..used);
                    match self.handle(frame).await {
                        Ok(Some(message)) => return Ok(Some(message)),
                        Ok(None) => {}
                        Err(err) => return Err(err),
                    }
                }
                Ok(None) => {
                    let mut chunk = [0u8; 8192];
                    let n = self.io.read(&mut chunk).await?;
                    if n == 0 {
                        self.closed = true;
                        return if self.partial.is_some() || !self.inbox.is_empty() {
                            Err(Error::Io(std::io::ErrorKind::UnexpectedEof.into()))
                        } else {
                            Ok(None)
                        };
                    }
                    self.inbox.extend_from_slice(&chunk[..n]);
                }
            }
        }
    }

    async fn fail(&mut self, err: ProtocolError) -> Error {
        if !self.close_sent {
            let _ = self
                .send_close(Some(CloseFrame {
                    code: err.close_code(),
                    reason: String::new(),
                }))
                .await;
        }
        self.closed = true;
        Error::Protocol(err)
    }

    async fn handle(&mut self, frame: Frame) -> Result<Option<Message>, Error> {
        match frame.opcode {
            Opcode::Ping => {
                if !self.close_sent {
                    self.write_frame(Opcode::Pong, &frame.payload).await?;
                }
                Ok(None)
            }
            Opcode::Pong => Ok(Some(Message::Pong(frame.payload))),
            Opcode::Close => {
                let parsed = match CloseFrame::parse(&frame.payload) {
                    Ok(parsed) => parsed,
                    Err(err) => return Err(self.fail(err).await),
                };
                if !self.close_sent {
                    // Echo the code, as the closing handshake asks.
                    self.send_close(parsed.clone().map(|f| CloseFrame {
                        code: f.code,
                        reason: String::new(),
                    }))
                    .await?;
                }
                self.closed = true;
                Ok(Some(Message::Close(parsed)))
            }
            Opcode::Text | Opcode::Binary => {
                if self.partial.is_some() {
                    return Err(self.fail(ProtocolError::InterruptedMessage).await);
                }
                if frame.fin {
                    return self.finish(frame.opcode, frame.payload).await.map(Some);
                }
                self.partial = Some((frame.opcode, frame.payload));
                Ok(None)
            }
            Opcode::Continuation => {
                let Some((kind, mut data)) = self.partial.take() else {
                    return Err(self.fail(ProtocolError::UnexpectedContinuation).await);
                };
                if data.len() + frame.payload.len() > self.config.max_message_size {
                    return Err(self.fail(ProtocolError::TooBig).await);
                }
                data.extend_from_slice(&frame.payload);
                if frame.fin {
                    return self.finish(kind, data).await.map(Some);
                }
                self.partial = Some((kind, data));
                Ok(None)
            }
        }
    }

    async fn finish(&mut self, kind: Opcode, data: Vec<u8>) -> Result<Message, Error> {
        if data.len() > self.config.max_message_size {
            return Err(self.fail(ProtocolError::TooBig).await);
        }
        if kind == Opcode::Text {
            return match String::from_utf8(data) {
                Ok(text) => Ok(Message::Text(text)),
                Err(_) => Err(self.fail(ProtocolError::InvalidUtf8).await),
            };
        }
        Ok(Message::Binary(data))
    }
}

/// Wraps an arbitrary stream as a client connection after a successful handshake.
pub const fn client<I: Io>(io: I, config: Config) -> WebSocket<I> {
    WebSocket::new(io, Role::Client, config)
}

/// Builds the headers of a client handshake for `key`.
#[must_use]
pub fn client_handshake_headers(key: &str) -> Vec<(&'static str, HeaderValue)> {
    vec![
        ("upgrade", HeaderValue::from_static("websocket")),
        ("connection", HeaderValue::from_static("Upgrade")),
        ("sec-websocket-version", HeaderValue::from_static("13")),
        (
            "sec-websocket-key",
            HeaderValue::from_str(key).unwrap_or_else(|_| HeaderValue::from_static("")),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncWriteExt, DuplexStream};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.append(*name, HeaderValue::from_str(value).unwrap());
        }
        map
    }

    fn check(pairs: &[(&str, &str)]) -> Result<String, HandshakeError> {
        validate_upgrade(&Method::GET, &headers(pairs))
    }

    const KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

    fn upgrade_headers() -> Vec<(&'static str, &'static str)> {
        vec![
            ("upgrade", "websocket"),
            ("connection", "keep-alive, Upgrade"),
            ("sec-websocket-version", "13"),
            ("sec-websocket-key", KEY),
        ]
    }

    #[test]
    fn the_rfc_handshake_example() {
        assert_eq!(accept_key(KEY), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
        assert_eq!(
            accept_key(&format!("  {KEY} ")),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        let generated = generate_key();
        assert_eq!(base64::STANDARD.decode(&generated).unwrap().len(), 16);
        assert_ne!(generated, generate_key());
    }

    #[test]
    fn validates_upgrade_requests() {
        assert_eq!(check(&upgrade_headers()).unwrap(), KEY);
        let without = |name: &str| -> Vec<(&str, &str)> {
            upgrade_headers()
                .into_iter()
                .filter(|(n, _)| *n != name)
                .collect()
        };
        assert_eq!(
            check(&without("upgrade")),
            Err(HandshakeError::MissingUpgrade)
        );
        assert_eq!(
            check(&without("connection")),
            Err(HandshakeError::MissingConnection)
        );
        assert_eq!(
            check(&without("sec-websocket-version")),
            Err(HandshakeError::UnsupportedVersion)
        );
        assert_eq!(
            check(&without("sec-websocket-key")),
            Err(HandshakeError::MissingKey)
        );
        assert_eq!(
            validate_upgrade(&Method::POST, &headers(&upgrade_headers())),
            Err(HandshakeError::NotGet)
        );
        let mut old = upgrade_headers();
        old.retain(|(n, _)| *n != "sec-websocket-version");
        old.push(("sec-websocket-version", "8"));
        assert_eq!(check(&old), Err(HandshakeError::UnsupportedVersion));
        let mut short = upgrade_headers();
        short.retain(|(n, _)| *n != "sec-websocket-key");
        short.push(("sec-websocket-key", "c2hvcnQ="));
        assert_eq!(check(&short), Err(HandshakeError::BadKey));
        for err in [
            HandshakeError::NotGet,
            HandshakeError::MissingUpgrade,
            HandshakeError::MissingConnection,
            HandshakeError::MissingKey,
            HandshakeError::BadKey,
            HandshakeError::UnsupportedVersion,
        ] {
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn accept_builds_the_switching_response() {
        let response = accept(
            &Method::GET,
            &headers(&upgrade_headers()),
            Config::default(),
            |_ws| async {},
        )
        .unwrap();
        assert_eq!(response.status, StatusCode::SWITCHING_PROTOCOLS);
        assert_eq!(
            response.headers.get("sec-websocket-accept").unwrap(),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert!(response.is_upgrade());
        assert!(
            accept(
                &Method::GET,
                &headers(&[]),
                Config::default(),
                |_ws| async {}
            )
            .is_err()
        );
    }

    #[test]
    fn rfc_6455_frame_examples() {
        // Unmasked "Hello" (section 5.7).
        let (frame, used) = decode_frame(&[0x81, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f], false, 1024)
            .unwrap()
            .unwrap();
        assert_eq!(
            (frame.fin, frame.opcode, frame.payload.as_slice(), used),
            (true, Opcode::Text, &b"Hello"[..], 7)
        );
        // Masked "Hello".
        let masked = [
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        let (frame, used) = decode_frame(&masked, true, 1024).unwrap().unwrap();
        assert_eq!((frame.payload.as_slice(), used), (&b"Hello"[..], 11));
        let mut out = Vec::new();
        encode_frame(
            &mut out,
            true,
            Opcode::Text,
            b"Hello",
            Some([0x37, 0xfa, 0x21, 0x3d]),
        );
        assert_eq!(out, masked);
        // Fragmented "Hel" + "lo".
        let (first, _) = decode_frame(&[0x01, 0x03, 0x48, 0x65, 0x6c], false, 1024)
            .unwrap()
            .unwrap();
        assert!(!first.fin && first.opcode == Opcode::Text);
        let (second, _) = decode_frame(&[0x80, 0x02, 0x6c, 0x6f], false, 1024)
            .unwrap()
            .unwrap();
        assert!(second.fin && second.opcode == Opcode::Continuation);
        // Ping with "Hello".
        let (ping, _) = decode_frame(&[0x89, 0x05, 0x48, 0x65, 0x6c, 0x6c, 0x6f], false, 1024)
            .unwrap()
            .unwrap();
        assert_eq!(ping.opcode, Opcode::Ping);
    }

    #[test]
    fn extended_lengths_round_trip() {
        for len in [0usize, 1, 125, 126, 127, 65_535, 65_536, 70_000] {
            let payload = vec![0xab; len];
            for mask in [None, Some([1, 2, 3, 4])] {
                let mut out = Vec::new();
                encode_frame(&mut out, true, Opcode::Binary, &payload, mask);
                let (frame, used) = decode_frame(&out, mask.is_some(), 1 << 20)
                    .unwrap()
                    .unwrap();
                assert_eq!((frame.payload, used), (payload.clone(), out.len()));
            }
        }
    }

    #[test]
    fn partial_frames_ask_for_more() {
        let mut out = Vec::new();
        encode_frame(&mut out, true, Opcode::Binary, &[7u8; 300], None);
        for cut in [0, 1, 2, 3, 10, out.len() - 1] {
            assert_eq!(
                decode_frame(&out[..cut], false, 1 << 20).unwrap(),
                None,
                "cut {cut}"
            );
        }
        let mut masked = Vec::new();
        encode_frame(&mut masked, true, Opcode::Text, b"abc", Some([9, 9, 9, 9]));
        for cut in [2, 3, 5, masked.len() - 1] {
            assert_eq!(
                decode_frame(&masked[..cut], true, 100).unwrap(),
                None,
                "cut {cut}"
            );
        }
        let mut huge = Vec::new();
        encode_frame(&mut huge, true, Opcode::Binary, &vec![0u8; 70_000], None);
        assert_eq!(decode_frame(&huge[..9], false, 1 << 20).unwrap(), None);
    }

    #[test]
    fn framing_violations_are_detected_at_the_header() {
        let cases: [(&[u8], bool, ProtocolError); 12] = [
            (&[0xc1, 0x00], false, ProtocolError::ReservedBits),
            (&[0xa1, 0x00], false, ProtocolError::ReservedBits),
            (&[0x91, 0x00], false, ProtocolError::ReservedBits),
            (&[0x83, 0x00], false, ProtocolError::UnknownOpcode),
            (&[0x8b, 0x00], false, ProtocolError::UnknownOpcode),
            (&[0x09, 0x00], false, ProtocolError::FragmentedControlFrame),
            (
                &[0x89, 0x7e, 0x00, 0x7e],
                false,
                ProtocolError::ControlFrameTooLong,
            ),
            (&[0x81, 0x80, 0, 0, 0, 0], false, ProtocolError::BadMasking),
            (&[0x81, 0x00], true, ProtocolError::BadMasking),
            (
                &[0x82, 0x7e, 0x00, 0x05],
                false,
                ProtocolError::NonMinimalLength,
            ),
            (
                &[0x82, 0x7f, 0, 0, 0, 0, 0, 0, 0, 5],
                false,
                ProtocolError::NonMinimalLength,
            ),
            (
                &[0x82, 0x7f, 0x80, 0, 0, 0, 0, 0, 0, 0],
                false,
                ProtocolError::NonMinimalLength,
            ),
        ];
        for (bytes, expect_masked, error) in cases {
            assert_eq!(
                decode_frame(bytes, expect_masked, 1 << 20),
                Err(error),
                "{bytes:02x?}"
            );
        }
        assert_eq!(
            decode_frame(&[0x82, 0x7e, 0x01, 0x00], false, 100),
            Err(ProtocolError::TooBig)
        );
        assert_eq!(
            decode_frame(
                &[0x82, 0x7f, 0, 0, 0, 1, 0, 0, 0, 0],
                false,
                usize::MAX >> 1
            )
            .map(|_| ()),
            Ok(())
        );
    }

    #[test]
    fn protocol_errors_map_to_close_codes() {
        assert_eq!(ProtocolError::InvalidUtf8.close_code(), 1007);
        assert_eq!(ProtocolError::InvalidClosePayload.close_code(), 1007);
        assert_eq!(ProtocolError::TooBig.close_code(), 1009);
        assert_eq!(ProtocolError::ReservedBits.close_code(), 1002);
        for err in [
            ProtocolError::ReservedBits,
            ProtocolError::UnknownOpcode,
            ProtocolError::FragmentedControlFrame,
            ProtocolError::ControlFrameTooLong,
            ProtocolError::BadMasking,
            ProtocolError::UnexpectedContinuation,
            ProtocolError::InterruptedMessage,
            ProtocolError::NonMinimalLength,
            ProtocolError::InvalidClosePayload,
            ProtocolError::InvalidCloseCode,
            ProtocolError::InvalidUtf8,
            ProtocolError::TooBig,
        ] {
            assert!(!err.to_string().is_empty());
        }
        assert!(Opcode::Ping.is_control() && !Opcode::Text.is_control());
    }

    #[test]
    fn close_payload_rules() {
        assert_eq!(CloseFrame::parse(&[]).unwrap(), None);
        assert_eq!(
            CloseFrame::parse(&[0x03, 0xe8]).unwrap(),
            Some(CloseFrame {
                code: 1000,
                reason: String::new()
            })
        );
        assert_eq!(
            CloseFrame::parse(b"\x03\xe9bye").unwrap().unwrap().reason,
            "bye"
        );
        assert_eq!(
            CloseFrame::parse(&[0x03]),
            Err(ProtocolError::InvalidClosePayload)
        );
        for code in [0u16, 999, 1004, 1005, 1006, 1012, 1015, 2999, 5000] {
            assert_eq!(
                CloseFrame::parse(&code.to_be_bytes()),
                Err(ProtocolError::InvalidCloseCode),
                "{code}"
            );
        }
        for code in [
            1000u16, 1001, 1002, 1003, 1007, 1008, 1009, 1010, 1011, 3000, 4999,
        ] {
            assert!(CloseFrame::parse(&code.to_be_bytes()).is_ok(), "{code}");
        }
        assert_eq!(
            CloseFrame::parse(&[0x03, 0xe8, 0xff]),
            Err(ProtocolError::InvalidUtf8)
        );
        assert_eq!(
            CloseFrame {
                code: 1001,
                reason: "away".into()
            }
            .payload(),
            b"\x03\xe9away"
        );
    }

    fn pair() -> (WebSocket<DuplexStream>, WebSocket<DuplexStream>) {
        let (a, b) = tokio::io::duplex(1 << 20);
        (
            WebSocket::new(a, Role::Client, Config::default()),
            WebSocket::new(b, Role::Server, Config::default()),
        )
    }

    #[tokio::test]
    async fn exchanges_text_and_binary_both_ways() {
        let (mut client, mut server) = pair();
        client.send(Message::Text("héllo".into())).await.unwrap();
        client.send(Message::Binary(vec![1, 2, 3])).await.unwrap();
        assert_eq!(
            server.recv().await.unwrap(),
            Some(Message::Text("héllo".into()))
        );
        assert_eq!(
            server.recv().await.unwrap(),
            Some(Message::Binary(vec![1, 2, 3]))
        );
        server.send(Message::Text("back".into())).await.unwrap();
        assert_eq!(
            client.recv().await.unwrap(),
            Some(Message::Text("back".into()))
        );
        assert!(format!("{client:?}").contains("Client"));
    }

    #[tokio::test]
    async fn pings_are_answered_and_pongs_surface() {
        let (mut client, mut server) = pair();
        client
            .send(Message::Ping(b"are you there".to_vec()))
            .await
            .unwrap();
        client.send(Message::Text("after".into())).await.unwrap();
        assert_eq!(
            server.recv().await.unwrap(),
            Some(Message::Text("after".into()))
        );
        assert_eq!(
            client.recv().await.unwrap(),
            Some(Message::Pong(b"are you there".to_vec()))
        );
        server.send(Message::Pong(vec![])).await.unwrap();
        assert_eq!(client.recv().await.unwrap(), Some(Message::Pong(vec![])));
        assert!(matches!(
            client.send(Message::Ping(vec![0; 126])).await,
            Err(Error::ControlTooLong)
        ));
        assert!(matches!(
            client.send(Message::Pong(vec![0; 126])).await,
            Err(Error::ControlTooLong)
        ));
    }

    #[tokio::test]
    async fn the_closing_handshake_completes_from_either_side() {
        let (mut client, mut server) = pair();
        client.close(1000, "done").await.unwrap();
        client.close(1000, "again").await.unwrap();
        assert!(matches!(
            client.send(Message::Text("late".into())).await,
            Err(Error::Closed)
        ));
        assert_eq!(
            server.recv().await.unwrap(),
            Some(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "done".into()
            })))
        );
        assert!(server.is_closed());
        assert_eq!(server.recv().await.unwrap(), None);
        // The server echoed the close; the client sees it and is done.
        assert_eq!(
            client.recv().await.unwrap(),
            Some(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: String::new()
            })))
        );
        assert_eq!(client.recv().await.unwrap(), None);
    }

    #[tokio::test]
    async fn an_empty_close_is_echoed_empty() {
        let (mut client, mut server) = pair();
        client.send(Message::Close(None)).await.unwrap();
        assert_eq!(server.recv().await.unwrap(), Some(Message::Close(None)));
        assert_eq!(client.recv().await.unwrap(), Some(Message::Close(None)));
    }

    #[tokio::test]
    async fn long_close_reasons_are_truncated_on_a_char_boundary() {
        let (mut client, mut server) = pair();
        client.close(1000, &"é".repeat(100)).await.unwrap();
        let Some(Message::Close(Some(frame))) = server.recv().await.unwrap() else {
            panic!("expected close")
        };
        assert!(frame.reason.len() <= 123 && frame.reason.chars().all(|c| c == 'é'));
        assert!(matches!(
            client
                .send(Message::Close(Some(CloseFrame {
                    code: 1000,
                    reason: "x".repeat(200)
                })))
                .await,
            Err(Error::Closed)
        ));
    }

    #[tokio::test]
    async fn oversized_close_payloads_are_refused() {
        let (mut client, _server) = pair();
        let result = client
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: "x".repeat(200),
            })))
            .await;
        assert!(matches!(result, Err(Error::ControlTooLong)));
    }

    async fn raw_server(frames: &[u8]) -> (WebSocket<DuplexStream>, DuplexStream) {
        let (a, mut b) = tokio::io::duplex(1 << 20);
        b.write_all(frames).await.unwrap();
        (
            WebSocket::new(
                a,
                Role::Server,
                Config {
                    max_message_size: 1000,
                    max_frame_size: 1000,
                },
            ),
            b,
        )
    }

    fn masked(fin: bool, opcode: Opcode, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        encode_frame(&mut out, fin, opcode, payload, Some([5, 6, 7, 8]));
        out
    }

    #[tokio::test]
    async fn reassembles_fragments_with_interleaved_control_frames() {
        let mut wire = masked(false, Opcode::Text, b"Hel");
        wire.extend(masked(true, Opcode::Ping, b"mid"));
        wire.extend(masked(false, Opcode::Continuation, b"lo "));
        wire.extend(masked(true, Opcode::Continuation, b"world"));
        let (mut ws, mut peer) = raw_server(&wire).await;
        assert_eq!(
            ws.recv().await.unwrap(),
            Some(Message::Text("Hello world".into()))
        );
        let mut pong = [0u8; 5];
        peer.read_exact(&mut pong).await.unwrap();
        assert_eq!(pong, [0x8a, 0x03, b'm', b'i', b'd']);
    }

    #[tokio::test]
    async fn utf8_is_checked_on_the_whole_message() {
        // "é" split across two fragments is fine.
        let mut wire = masked(false, Opcode::Text, &[0xc3]);
        wire.extend(masked(true, Opcode::Continuation, &[0xa9]));
        let (mut ws, _peer) = raw_server(&wire).await;
        assert_eq!(ws.recv().await.unwrap(), Some(Message::Text("é".into())));
        // Invalid bytes are a 1007.
        let (mut ws, mut peer) = raw_server(&masked(true, Opcode::Text, &[0xff, 0xfe])).await;
        assert!(matches!(
            ws.recv().await,
            Err(Error::Protocol(ProtocolError::InvalidUtf8))
        ));
        let mut close = [0u8; 4];
        peer.read_exact(&mut close).await.unwrap();
        assert_eq!(close, [0x88, 0x02, 0x03, 0xef]);
        assert!(ws.is_closed());
        assert_eq!(ws.recv().await.unwrap(), None);
    }

    #[tokio::test]
    async fn sequencing_violations_close_with_1002() {
        let cases: Vec<Vec<u8>> = vec![
            masked(true, Opcode::Continuation, b"orphan"),
            [
                masked(false, Opcode::Text, b"a"),
                masked(true, Opcode::Text, b"b"),
            ]
            .concat(),
            [
                masked(false, Opcode::Binary, b"a"),
                masked(true, Opcode::Binary, b"b"),
            ]
            .concat(),
            masked(true, Opcode::Close, &[0x03]),
            masked(true, Opcode::Close, &[0x03, 0xed]),
            vec![0xc1, 0x80, 0, 0, 0, 0],
        ];
        for wire in cases {
            let (mut ws, mut peer) = raw_server(&wire).await;
            let err = ws.recv().await.unwrap_err();
            assert!(matches!(err, Error::Protocol(_)), "{wire:02x?}: {err}");
            let mut head = [0u8; 2];
            peer.read_exact(&mut head).await.unwrap();
            assert_eq!(head[0], 0x88);
        }
    }

    #[tokio::test]
    async fn size_limits_close_with_1009() {
        let (mut ws, mut peer) = raw_server(&masked(true, Opcode::Binary, &vec![0u8; 1001])).await;
        assert!(matches!(
            ws.recv().await,
            Err(Error::Protocol(ProtocolError::TooBig))
        ));
        let mut close = [0u8; 4];
        peer.read_exact(&mut close).await.unwrap();
        assert_eq!(close, [0x88, 0x02, 0x03, 0xf1]);

        let mut wire = masked(false, Opcode::Binary, &vec![0u8; 600]);
        wire.extend(masked(true, Opcode::Continuation, &vec![0u8; 600]));
        let (mut ws, _peer) = raw_server(&wire).await;
        assert!(matches!(
            ws.recv().await,
            Err(Error::Protocol(ProtocolError::TooBig))
        ));

        // A single frame within the frame limit but a message over the message limit.
        let config = Config {
            max_message_size: 4,
            max_frame_size: 100,
        };
        let (a, mut b) = tokio::io::duplex(1024);
        b.write_all(&masked(true, Opcode::Text, b"12345"))
            .await
            .unwrap();
        let mut ws = WebSocket::new(a, Role::Server, config);
        assert!(matches!(
            ws.recv().await,
            Err(Error::Protocol(ProtocolError::TooBig))
        ));
    }

    #[tokio::test]
    async fn eof_in_the_middle_of_a_message_is_an_error_but_clean_eof_is_none() {
        let (mut ws, peer) = raw_server(&masked(false, Opcode::Text, b"half")).await;
        drop(peer);
        assert!(matches!(ws.recv().await, Err(Error::Io(_))));
        let (mut ws, peer) = raw_server(&[]).await;
        drop(peer);
        assert_eq!(ws.recv().await.unwrap(), None);
        assert!(ws.is_closed());
    }

    #[tokio::test]
    async fn frames_arriving_in_dribs_are_assembled() {
        let (a, mut b) = tokio::io::duplex(1024);
        let mut ws = WebSocket::new(a, Role::Server, Config::default());
        let wire = masked(true, Opcode::Text, b"slowly");
        let task = tokio::spawn(async move {
            for byte in wire {
                b.write_all(&[byte]).await.unwrap();
                tokio::task::yield_now().await;
            }
            b
        });
        assert_eq!(
            ws.recv().await.unwrap(),
            Some(Message::Text("slowly".into()))
        );
        let _ = task.await;
    }

    #[tokio::test]
    async fn pings_after_our_close_are_not_answered() {
        let (mut ws, mut peer) = raw_server(&masked(true, Opcode::Ping, b"x")).await;
        ws.close(1000, "").await.unwrap();
        peer.write_all(&masked(true, Opcode::Close, &[0x03, 0xe8]))
            .await
            .unwrap();
        assert!(matches!(ws.recv().await.unwrap(), Some(Message::Close(_))));
        let mut written = Vec::new();
        peer.shutdown().await.unwrap();
        drop(ws);
        peer.read_to_end(&mut written).await.unwrap();
        assert_eq!(
            written,
            [0x88, 0x02, 0x03, 0xe8],
            "only our own close was written"
        );
    }

    #[tokio::test]
    async fn error_display_and_sources() {
        use std::error::Error as _;
        let io = Error::from(std::io::Error::other("boom"));
        assert!(io.to_string().contains("boom") && io.source().is_some());
        let proto = Error::Protocol(ProtocolError::TooBig);
        assert!(proto.to_string().contains("too big") && proto.source().is_some());
        assert!(Error::Closed.to_string().contains("closed") && Error::Closed.source().is_none());
        assert!(Error::ControlTooLong.to_string().contains("125"));
    }

    #[test]
    fn client_handshake_headers_carry_the_key() {
        let headers = client_handshake_headers(KEY);
        assert!(
            headers
                .iter()
                .any(|(n, v)| *n == "sec-websocket-key" && *v == KEY)
        );
        assert!(
            headers
                .iter()
                .any(|(n, v)| *n == "sec-websocket-version" && *v == "13")
        );
    }

    #[tokio::test]
    async fn the_client_helper_masks_frames() {
        let (a, mut b) = tokio::io::duplex(1024);
        let mut ws = client(a, Config::default());
        ws.send(Message::Text("x".into())).await.unwrap();
        let mut head = [0u8; 2];
        b.read_exact(&mut head).await.unwrap();
        assert_eq!(head[1] & 0x80, 0x80);
    }
}
