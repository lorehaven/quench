//! Async SMTP submission (RFC 6409 over RFC 5321): connect, secure, log in,
//! send one message, hang up.
//!
//! One connection per message, no pooling: this is for transactional mail
//! (a verification link, a reset link), where a handshake per send is cheap
//! next to the human waiting for the email.

use crate::error::{Error, Reply};
use crate::message::{Envelope, Message};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rustls::pki_types::{CertificateDer, ServerName};
use std::fmt;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;

/// A reply line longer than this, or a reply with more lines, is not SMTP.
const MAX_LINE: usize = 4096;
const MAX_REPLY_LINES: usize = 100;

/// How the connection is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    /// TLS from the first byte - port 465 ("SMTPS").
    Tls,
    /// Plain first, upgraded with `STARTTLS` before anything sensitive - 587.
    StartTls,
    /// No encryption. Only for a trusted local hop; refuses to log in unless
    /// [`MailerBuilder::allow_plaintext_auth`] says so.
    None,
}

impl Security {
    fn default_port(self) -> u16 {
        match self {
            Security::Tls => 465,
            Security::StartTls => 587,
            Security::None => 25,
        }
    }
}

#[derive(Clone)]
struct Credentials {
    username: String,
    password: String,
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("username", &self.username)
            .field("password", &"<redacted>")
            .finish()
    }
}

/// The server's acceptance of a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub reply: Reply,
}

impl Response {
    /// Server text such as `2.0.0 Message queued as 1234` - what to log.
    pub fn message(&self) -> String {
        self.reply.message()
    }
}

#[derive(Debug)]
pub struct MailerBuilder {
    host: String,
    port: Option<u16>,
    security: Security,
    tls_server_name: Option<String>,
    credentials: Option<Credentials>,
    hello_name: String,
    timeout: Duration,
    extra_roots: Vec<CertificateDer<'static>>,
    allow_plaintext_auth: bool,
}

impl MailerBuilder {
    pub fn security(mut self, security: Security) -> Self {
        self.security = security;
        self
    }

    /// Defaults to 465, 587 or 25 by [`Security`].
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// The name the server's certificate must be valid for, when that is not
    /// the host connected to - e.g. connecting to an in-cluster service name
    /// while the certificate is for the public hostname.
    pub fn tls_server_name(mut self, name: &str) -> Self {
        self.tls_server_name = Some(name.to_string());
        self
    }

    pub fn credentials(mut self, username: &str, password: &str) -> Self {
        self.credentials = Some(Credentials {
            username: username.to_string(),
            password: password.to_string(),
        });
        self
    }

    /// The name announced in `EHLO`. Defaults to `localhost`.
    pub fn hello_name(mut self, name: &str) -> Self {
        self.hello_name = name.to_string();
        self
    }

    /// Limit for connecting and for each wait on the server. Default 30 s.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Trust one more root, given as a PEM certificate, besides the built-in
    /// public roots - for a private CA or a self-signed server.
    pub fn add_root_certificate_pem(mut self, pem: &str) -> Result<Self, Error> {
        let mut found = false;
        for cert in pem_certificates(pem)? {
            self.extra_roots.push(cert);
            found = true;
        }
        if found {
            Ok(self)
        } else {
            Err(Error::Config("no certificate found in the PEM".into()))
        }
    }

    /// Send credentials over an unencrypted connection. Off by default.
    pub fn allow_plaintext_auth(mut self, allow: bool) -> Self {
        self.allow_plaintext_auth = allow;
        self
    }

    pub fn build(self) -> Result<Mailer, Error> {
        if self.host.trim().is_empty() {
            return Err(Error::Config("SMTP host is empty".into()));
        }
        if self
            .host
            .chars()
            .any(|c| c.is_control() || c.is_whitespace())
            || self
                .hello_name
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || self.hello_name.is_empty()
        {
            return Err(Error::Config(
                "host and EHLO name must not contain whitespace".into(),
            ));
        }
        if self.credentials.is_some()
            && self.security == Security::None
            && !self.allow_plaintext_auth
        {
            return Err(Error::Config(
                "refusing to send credentials without TLS (see allow_plaintext_auth)".into(),
            ));
        }

        let mut roots =
            rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        for cert in &self.extra_roots {
            roots
                .add(cert.clone())
                .map_err(|err| Error::Config(format!("unusable root certificate: {err}")))?;
        }
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|err| Error::Tls(err.to_string()))?
        .with_root_certificates(roots)
        .with_no_client_auth();

        let name = self
            .tls_server_name
            .clone()
            .unwrap_or_else(|| self.host.clone());
        let server_name = ServerName::try_from(name.clone())
            .map_err(|_| Error::Config(format!("invalid TLS server name {name:?}")))?;

        Ok(Mailer {
            inner: Arc::new(Inner {
                port: self.port.unwrap_or_else(|| self.security.default_port()),
                host: self.host,
                security: self.security,
                credentials: self.credentials,
                hello_name: self.hello_name,
                timeout: self.timeout,
                connector: TlsConnector::from(Arc::new(tls)),
                server_name,
            }),
        })
    }
}

fn pem_certificates(pem: &str) -> Result<Vec<CertificateDer<'static>>, Error> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut certs = Vec::new();
    let mut rest = pem;
    while let Some(start) = rest.find(BEGIN) {
        let after = &rest[start + BEGIN.len()..];
        let end = after
            .find(END)
            .ok_or_else(|| Error::Config("unterminated PEM certificate".into()))?;
        let body: String = after[..end].split_whitespace().collect();
        let der = STANDARD
            .decode(body)
            .map_err(|err| Error::Config(format!("invalid PEM certificate: {err}")))?;
        certs.push(CertificateDer::from(der));
        rest = &after[end + END.len()..];
    }
    Ok(certs)
}

struct Inner {
    host: String,
    port: u16,
    security: Security,
    credentials: Option<Credentials>,
    hello_name: String,
    timeout: Duration,
    connector: TlsConnector,
    server_name: ServerName<'static>,
}

/// A configured SMTP submission client. Cheap to clone.
#[derive(Clone)]
pub struct Mailer {
    inner: Arc<Inner>,
}

impl fmt::Debug for Mailer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailer")
            .field("host", &self.inner.host)
            .field("port", &self.inner.port)
            .field("security", &self.inner.security)
            .finish_non_exhaustive()
    }
}

impl Mailer {
    /// Connects to `host` over TLS (port 465) unless told otherwise.
    pub fn builder(host: &str) -> MailerBuilder {
        MailerBuilder {
            host: host.to_string(),
            port: None,
            security: Security::Tls,
            tls_server_name: None,
            credentials: None,
            hello_name: "localhost".to_string(),
            timeout: Duration::from_secs(30),
            extra_roots: Vec::new(),
            allow_plaintext_auth: false,
        }
    }

    pub async fn send(&self, message: &Message) -> Result<Response, Error> {
        self.send_raw(&message.envelope(), &message.formatted())
            .await
    }

    /// Sends already-formatted message bytes (CRLF line endings) to the
    /// envelope's recipients.
    pub async fn send_raw(&self, envelope: &Envelope, data: &[u8]) -> Result<Response, Error> {
        let mut session = self.open().await?;
        let result = session.deliver(envelope, data).await;
        session.quit().await;
        result
    }

    /// Connects, secures and logs in, then hangs up without sending anything -
    /// a startup check that the settings work.
    pub async fn test_connection(&self) -> Result<(), Error> {
        let mut session = self.open().await?;
        session.quit().await;
        Ok(())
    }

    /// Greeting, EHLO, STARTTLS if configured, and login: a session ready to send.
    async fn open(&self) -> Result<Session, Error> {
        let inner = &self.inner;
        let tcp = timeout(
            inner.timeout,
            TcpStream::connect((inner.host.as_str(), inner.port)),
        )
        .await
        .map_err(|_| Error::Timeout("the connection"))??;

        let stream = if inner.security == Security::Tls {
            Stream::Tls(Box::new(handshake(inner, tcp).await?))
        } else {
            Stream::Plain(tcp)
        };
        let mut session = Session {
            conn: Connection::new(stream, inner.timeout),
            inner: inner.clone(),
            caps: Capabilities::default(),
            secure: inner.security == Security::Tls,
        };

        session.expect(220, "greeting").await?;
        session.ehlo().await?;
        if inner.security == Security::StartTls {
            session.start_tls().await?;
        }
        if inner.credentials.is_some() {
            session.authenticate().await?;
        }
        Ok(session)
    }
}

async fn handshake(inner: &Inner, tcp: TcpStream) -> Result<TlsStream<TcpStream>, Error> {
    timeout(
        inner.timeout,
        inner.connector.connect(inner.server_name.clone(), tcp),
    )
    .await
    .map_err(|_| Error::Timeout("the TLS handshake"))?
    .map_err(|err| Error::Tls(err.to_string()))
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    /// Parked here while the plain socket is being upgraded to TLS; any I/O
    /// on it is an error.
    Empty,
}

fn not_connected() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "stream is being upgraded")
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_read(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
            Stream::Empty => Poll::Ready(Err(not_connected())),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_write(cx, buf),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
            Stream::Empty => Poll::Ready(Err(not_connected())),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_flush(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
            Stream::Empty => Poll::Ready(Err(not_connected())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Plain(s) => Pin::new(s).poll_shutdown(cx),
            Stream::Tls(s) => Pin::new(s.as_mut()).poll_shutdown(cx),
            Stream::Empty => Poll::Ready(Ok(())),
        }
    }
}

/// The stream plus what has been read from it but not yet consumed.
struct Connection {
    stream: Stream,
    buffer: Vec<u8>,
    timeout: Duration,
}

impl Connection {
    fn new(stream: Stream, timeout: Duration) -> Self {
        Self {
            stream,
            buffer: Vec::new(),
            timeout,
        }
    }

    async fn write(&mut self, bytes: &[u8]) -> Result<(), Error> {
        timeout(self.timeout, async {
            self.stream.write_all(bytes).await?;
            self.stream.flush().await
        })
        .await
        .map_err(|_| Error::Timeout("the server to accept data"))??;
        Ok(())
    }

    async fn line(&mut self) -> Result<String, Error> {
        loop {
            if let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
                let mut line: Vec<u8> = self.buffer.drain(..=end).collect();
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                return String::from_utf8(line)
                    .map_err(|_| Error::Protocol("reply is not valid UTF-8".into()));
            }
            if self.buffer.len() > MAX_LINE {
                return Err(Error::Protocol("reply line too long".into()));
            }
            let mut chunk = [0u8; 1024];
            let n = timeout(self.timeout, self.stream.read(&mut chunk))
                .await
                .map_err(|_| Error::Timeout("the server's reply"))??;
            if n == 0 {
                return Err(Error::Protocol("connection closed by the server".into()));
            }
            self.buffer.extend_from_slice(&chunk[..n]);
        }
    }

    /// One complete, possibly multi-line, reply.
    async fn reply(&mut self) -> Result<Reply, Error> {
        let mut lines = Vec::new();
        let mut code = 0u16;
        loop {
            let line = self.line().await?;
            let (head, text, last) = parse_reply_line(&line)?;
            if lines.is_empty() {
                code = head;
            } else if head != code {
                return Err(Error::Protocol("reply code changed mid-reply".into()));
            }
            lines.push(text.to_string());
            if last {
                break;
            }
            if lines.len() >= MAX_REPLY_LINES {
                return Err(Error::Protocol("reply has too many lines".into()));
            }
        }
        let enhanced = lines.first().and_then(|l| enhanced_status(l));
        let lines = match &enhanced {
            Some(status) => lines
                .into_iter()
                .map(|l| {
                    l.strip_prefix(status.as_str())
                        .unwrap_or(&l)
                        .trim()
                        .to_string()
                })
                .collect(),
            None => lines,
        };
        Ok(Reply {
            code,
            enhanced,
            lines,
        })
    }
}

/// `250-text` / `250 text` -> (250, "text", is_last).
fn parse_reply_line(line: &str) -> Result<(u16, &str, bool), Error> {
    let bad = || Error::Protocol(format!("malformed reply line {line:?}"));
    let bytes = line.as_bytes();
    if bytes.len() < 3 || !bytes[..3].iter().all(u8::is_ascii_digit) {
        return Err(bad());
    }
    let code: u16 = line[..3].parse().map_err(|_| bad())?;
    match bytes.get(3) {
        None => Ok((code, "", true)),
        Some(b' ') => Ok((code, &line[4..], true)),
        Some(b'-') => Ok((code, &line[4..], false)),
        Some(_) => Err(bad()),
    }
}

/// The RFC 3463 status leading a reply's text, e.g. `5.7.1`.
fn enhanced_status(text: &str) -> Option<String> {
    let token = text.split(' ').next()?;
    let parts: Vec<&str> = token.split('.').collect();
    let ok = parts.len() == 3
        && matches!(parts[0], "2" | "4" | "5")
        && parts[1..]
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()));
    ok.then(|| token.to_string())
}

#[derive(Default)]
struct Capabilities {
    starttls: bool,
    auth: Vec<String>,
    size: Option<usize>,
}

impl Capabilities {
    fn parse(reply: &Reply) -> Self {
        let mut caps = Capabilities::default();
        // The first line is the greeting, the rest are extensions.
        for line in reply.lines.iter().skip(1) {
            let mut words = line.split_whitespace();
            match words.next().map(str::to_ascii_uppercase).as_deref() {
                Some("STARTTLS") => caps.starttls = true,
                Some("AUTH") => caps.auth = words.map(str::to_ascii_uppercase).collect(),
                Some("SIZE") => caps.size = words.next().and_then(|n| n.parse().ok()),
                _ => {}
            }
        }
        caps
    }
}

struct Session {
    conn: Connection,
    inner: Arc<Inner>,
    caps: Capabilities,
    secure: bool,
}

impl Session {
    async fn command(&mut self, line: &str) -> Result<(), Error> {
        self.conn.write(format!("{line}\r\n").as_bytes()).await
    }

    /// Reads a reply and requires `code`, otherwise reports `stage`.
    async fn expect(&mut self, code: u16, stage: &'static str) -> Result<Reply, Error> {
        let reply = self.conn.reply().await?;
        if reply.code == code {
            Ok(reply)
        } else {
            Err(Error::Rejected { stage, reply })
        }
    }

    async fn ehlo(&mut self) -> Result<(), Error> {
        let hello = self.inner.hello_name.clone();
        self.command(&format!("EHLO {hello}")).await?;
        let reply = self.conn.reply().await?;
        if reply.code == 250 {
            self.caps = Capabilities::parse(&reply);
            return Ok(());
        }
        // A server that has never heard of EHLO: fall back to HELO, which
        // offers no extensions - so no STARTTLS and no AUTH.
        self.command(&format!("HELO {hello}")).await?;
        self.expect(250, "HELO").await?;
        self.caps = Capabilities::default();
        Ok(())
    }

    async fn start_tls(&mut self) -> Result<(), Error> {
        if !self.caps.starttls {
            return Err(Error::Unsupported(
                "the server does not offer STARTTLS".into(),
            ));
        }
        self.command("STARTTLS").await?;
        self.expect(220, "STARTTLS").await?;
        // Anything the server sent after its 220 was sent in the clear and is
        // not to be trusted once TLS is up (the "STARTTLS injection" attack).
        if !self.conn.buffer.is_empty() {
            return Err(Error::Protocol(
                "server sent data before the TLS handshake".into(),
            ));
        }
        let tcp = match std::mem::replace(&mut self.conn.stream, Stream::Empty) {
            Stream::Plain(tcp) => tcp,
            _ => {
                return Err(Error::Protocol(
                    "cannot upgrade this connection to TLS".into(),
                ));
            }
        };
        self.conn.stream = Stream::Tls(Box::new(handshake(&self.inner, tcp).await?));
        self.secure = true;
        // Everything learned before TLS is discarded and asked again.
        self.ehlo().await
    }

    async fn authenticate(&mut self) -> Result<(), Error> {
        let Some(credentials) = self.inner.credentials.clone() else {
            return Ok(());
        };
        if !self.secure && !self.plaintext_auth_allowed() {
            return Err(Error::Unsupported(
                "refusing to send credentials without TLS".into(),
            ));
        }
        let offers = |mechanism: &str| self.caps.auth.iter().any(|m| m == mechanism);
        if offers("PLAIN") {
            let token = STANDARD.encode(format!(
                "\0{}\0{}",
                credentials.username, credentials.password
            ));
            self.command(&format!("AUTH PLAIN {token}")).await?;
            self.auth_result().await
        } else if offers("LOGIN") {
            self.command("AUTH LOGIN").await?;
            self.auth_step(334).await?;
            self.command(&STANDARD.encode(&credentials.username))
                .await?;
            self.auth_step(334).await?;
            self.command(&STANDARD.encode(&credentials.password))
                .await?;
            self.auth_result().await
        } else {
            Err(Error::Unsupported(format!(
                "the server offers no supported login mechanism (offers: {})",
                if self.caps.auth.is_empty() {
                    "none".to_string()
                } else {
                    self.caps.auth.join(", ")
                }
            )))
        }
    }

    fn plaintext_auth_allowed(&self) -> bool {
        // `build` already refused Security::None without the opt-in, so a
        // plaintext session that reaches here has it.
        self.inner.security == Security::None
    }

    async fn auth_step(&mut self, code: u16) -> Result<(), Error> {
        let reply = self.conn.reply().await?;
        if reply.code == code {
            Ok(())
        } else {
            Err(Error::Auth(reply))
        }
    }

    async fn auth_result(&mut self) -> Result<(), Error> {
        self.auth_step(235).await
    }

    async fn deliver(&mut self, envelope: &Envelope, data: &[u8]) -> Result<Response, Error> {
        if envelope.to.is_empty() {
            return Err(Error::Config("no recipients".into()));
        }
        if let Some(limit) = self.caps.size
            && data.len() > limit
        {
            return Err(Error::TooLarge {
                size: data.len(),
                limit,
            });
        }

        self.command(&format!("MAIL FROM:<{}>", envelope.from))
            .await?;
        self.expect(250, "MAIL FROM").await?;
        for recipient in &envelope.to {
            self.command(&format!("RCPT TO:<{recipient}>")).await?;
            let reply = self.conn.reply().await?;
            if !matches!(reply.code, 250 | 251) {
                return Err(Error::Rejected {
                    stage: "RCPT TO",
                    reply,
                });
            }
        }

        self.command("DATA").await?;
        self.expect(354, "DATA").await?;
        self.conn.write(&dot_stuff(data)).await?;
        let reply = self.expect(250, "end of DATA").await?;
        Ok(Response { reply })
    }

    /// Polite goodbye; the message is already accepted or refused, so any
    /// failure here is not worth reporting.
    async fn quit(&mut self) {
        let _ = self.command("QUIT").await;
        let _ = timeout(Duration::from_secs(2), self.conn.reply()).await;
        let _ = self.conn.stream.shutdown().await;
    }
}

/// The DATA payload: a leading `.` on any line is doubled so it cannot end the
/// message early, a final CRLF is ensured, and the `.` terminator appended.
#[doc(hidden)]
pub fn dot_stuff(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut line_start = true;
    for &byte in data {
        if line_start && byte == b'.' {
            out.push(b'.');
        }
        out.push(byte);
        line_start = byte == b'\n';
    }
    if !out.ends_with(b"\r\n") {
        out.extend_from_slice(b"\r\n");
    }
    out.extend_from_slice(b".\r\n");
    out
}
