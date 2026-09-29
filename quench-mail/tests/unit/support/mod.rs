//! A scriptable in-process SMTP server for the client tests: real sockets, real
//! TLS (with the throwaway certificate in `tests/fixtures`), records what it got.
#![allow(dead_code)]

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;

pub const CERT_PEM: &str = include_str!("../../fixtures/localhost.crt");
const KEY_PEM: &str = include_str!("../../fixtures/localhost.key");

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tls {
    None,
    Implicit,
    Upgrade,
}

#[derive(Clone)]
pub struct Behavior {
    pub tls: Tls,
    pub auth_mechanisms: Vec<&'static str>,
    /// Required login; `None` accepts anyone.
    pub credentials: Option<(&'static str, &'static str)>,
    pub size: Option<usize>,
    /// Replies to `RCPT TO` for addresses containing `bad`.
    pub bad_rcpt_reply: &'static str,
    pub data_reply: &'static str,
    /// Accept the connection and never say a word.
    pub silent: bool,
    /// Hang up right after the greeting.
    pub hang_up: bool,
    /// Append this to the `220` that answers STARTTLS, in the clear.
    pub inject_after_starttls: Option<&'static str>,
    /// Do not advertise STARTTLS even when it would work.
    pub hide_starttls: bool,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            tls: Tls::None,
            auth_mechanisms: vec!["PLAIN", "LOGIN"],
            credentials: None,
            size: None,
            bad_rcpt_reply: "550 5.1.1 no such mailbox",
            data_reply: "250 2.0.0 queued as TEST1",
            silent: false,
            hang_up: false,
            inject_after_starttls: None,
            hide_starttls: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Received {
    pub helo: String,
    pub authed_as: Option<String>,
    pub from: String,
    pub rcpt: Vec<String>,
    pub data: String,
    pub used_tls: bool,
    /// Every command line seen, for asserting what was (not) sent.
    pub commands: Vec<String>,
}

pub struct FakeServer {
    pub addr: SocketAddr,
    pub log: Arc<Mutex<Vec<Received>>>,
}

impl FakeServer {
    /// A session is logged when its connection ends, which can trail the
    /// client's return by a moment - wait for `count` of them.
    pub async fn wait_for(&self, count: usize) {
        for _ in 0..200 {
            if self.log.lock().unwrap().len() >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("expected {count} finished sessions");
    }

    pub async fn last(&self) -> Received {
        self.wait_for(1).await;
        self.log.lock().unwrap().last().cloned().unwrap()
    }

    pub fn sessions(&self) -> usize {
        self.log.lock().unwrap().len()
    }
}

fn acceptor() -> TlsAcceptor {
    let cert_body: String = CERT_PEM
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let key_body: String = KEY_PEM
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    let cert = CertificateDer::from(STANDARD.decode(cert_body).unwrap());
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(STANDARD.decode(key_body).unwrap()));
    let config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert], key)
    .unwrap();
    TlsAcceptor::from(Arc::new(config))
}

pub async fn start(behavior: Behavior) -> FakeServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let log = Arc::new(Mutex::new(Vec::new()));
    let shared = log.clone();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else {
                return;
            };
            let behavior = behavior.clone();
            let log = shared.clone();
            tokio::spawn(async move {
                let mut received = Received::default();
                serve(tcp, &behavior, &mut received).await;
                log.lock().unwrap().push(received);
            });
        }
    });
    FakeServer { addr, log }
}

async fn serve(tcp: TcpStream, behavior: &Behavior, received: &mut Received) {
    if behavior.silent {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        return;
    }
    if behavior.tls == Tls::Implicit {
        let Ok(mut tls) = acceptor().accept(tcp).await else {
            return;
        };
        received.used_tls = true;
        let _ = converse(&mut tls, behavior, received, true).await;
        return;
    }

    let mut tcp = tcp;
    if let Outcome::Upgrade = converse(&mut tcp, behavior, received, false).await {
        let Ok(mut tls) = acceptor().accept(tcp).await else {
            return;
        };
        received.used_tls = true;
        let _ = converse(&mut tls, behavior, received, true).await;
    }
}

enum Outcome {
    Done,
    Upgrade,
}

async fn converse<S>(
    stream: &mut S,
    behavior: &Behavior,
    received: &mut Received,
    tls_active: bool,
) -> Outcome
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let mut reader = BufReader::new(&mut *stream);
    // Only the first leg greets; after STARTTLS the client speaks first.
    if !tls_active || behavior.tls == Tls::Implicit {
        if say(reader.get_mut(), "220 fake.test ESMTP ready")
            .await
            .is_err()
        {
            return Outcome::Done;
        }
        if behavior.hang_up {
            return Outcome::Done;
        }
    }
    let mut authed = received.authed_as.clone();
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return Outcome::Done,
            Ok(_) => {}
        }
        let command = line.trim_end().to_string();
        received.commands.push(command.clone());
        let upper = command.to_ascii_uppercase();
        let out = reader.get_mut();

        let sent = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            received.helo = command[5..].to_string();
            let mut lines = vec!["250-fake.test greets you".to_string()];
            if let Some(size) = behavior.size {
                lines.push(format!("250-SIZE {size}"));
            }
            let can_starttls =
                behavior.tls == Tls::Upgrade && !tls_active && !behavior.hide_starttls;
            if can_starttls {
                lines.push("250-STARTTLS".to_string());
            }
            let mechanisms = behavior.auth_mechanisms.join(" ");
            // Real servers offer AUTH only on a secured link; the plain-TLS-less
            // tests opt in by simply not requiring credentials.
            if !mechanisms.is_empty() {
                lines.push(format!("250-AUTH {mechanisms}"));
            }
            lines.push("250 8BITMIME".to_string());
            say_all(out, &lines).await
        } else if upper == "STARTTLS" {
            if behavior.tls != Tls::Upgrade {
                say(out, "502 5.5.1 not supported").await
            } else {
                let mut reply = String::from("220 2.0.0 ready to start TLS");
                if let Some(extra) = behavior.inject_after_starttls {
                    reply.push_str("\r\n");
                    reply.push_str(extra);
                }
                let _ = say(out, &reply).await;
                return Outcome::Upgrade;
            }
        } else if upper.starts_with("AUTH PLAIN") {
            let token = command["AUTH PLAIN".len()..].trim();
            let decoded = STANDARD.decode(token).unwrap_or_default();
            let parts: Vec<&[u8]> = decoded.split(|&b| b == 0).collect();
            let (user, pass) = match parts.as_slice() {
                [_, user, pass] => (
                    String::from_utf8_lossy(user).to_string(),
                    String::from_utf8_lossy(pass).to_string(),
                ),
                _ => (String::new(), String::new()),
            };
            check_login(out, behavior, &mut authed, user, pass).await
        } else if upper == "AUTH LOGIN" {
            let _ = say(out, "334 VXNlcm5hbWU6").await;
            let user = read_b64(&mut reader).await;
            let _ = say(reader.get_mut(), "334 UGFzc3dvcmQ6").await;
            let pass = read_b64(&mut reader).await;
            let out = reader.get_mut();
            check_login(out, behavior, &mut authed, user, pass).await
        } else if let Some(rest) = upper.strip_prefix("MAIL FROM:") {
            let _ = rest;
            received.authed_as = authed.clone();
            received.from = angle(&command);
            say(out, "250 2.1.0 ok").await
        } else if upper.starts_with("RCPT TO:") {
            let address = angle(&command);
            if address.contains("bad") {
                say(out, behavior.bad_rcpt_reply).await
            } else {
                received.rcpt.push(address);
                say(out, "250 2.1.5 ok").await
            }
        } else if upper == "DATA" {
            let _ = say(out, "354 go ahead").await;
            let data = read_data(&mut reader).await;
            received.data = data;
            let out = reader.get_mut();
            say(out, behavior.data_reply).await
        } else if upper == "QUIT" {
            let _ = say(out, "221 2.0.0 bye").await;
            return Outcome::Done;
        } else if upper == "RSET" {
            say(out, "250 ok").await
        } else {
            say(out, "500 5.5.2 unknown command").await
        };
        received.authed_as = authed.clone();
        if sent.is_err() {
            return Outcome::Done;
        }
    }
}

async fn check_login<W: AsyncWrite + Unpin>(
    out: &mut W,
    behavior: &Behavior,
    authed: &mut Option<String>,
    user: String,
    pass: String,
) -> std::io::Result<()> {
    let ok = match behavior.credentials {
        Some((u, p)) => user == u && pass == p,
        None => true,
    };
    if ok {
        *authed = Some(user);
        say(out, "235 2.7.0 authenticated").await
    } else {
        say(out, "535 5.7.8 authentication credentials invalid").await
    }
}

fn angle(command: &str) -> String {
    let start = command.find('<').map_or(0, |i| i + 1);
    let end = command.rfind('>').unwrap_or(command.len());
    command[start..end].to_string()
}

async fn say<W: AsyncWrite + Unpin>(out: &mut W, text: &str) -> std::io::Result<()> {
    out.write_all(format!("{text}\r\n").as_bytes()).await?;
    out.flush().await
}

async fn say_all<W: AsyncWrite + Unpin>(out: &mut W, lines: &[String]) -> std::io::Result<()> {
    for line in lines {
        say(out, line).await?;
    }
    Ok(())
}

async fn read_b64<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> String {
    let mut line = String::new();
    let _ = reader.read_line(&mut line).await;
    String::from_utf8_lossy(&STANDARD.decode(line.trim()).unwrap_or_default()).to_string()
}

/// Reads until the lone `.` line, undoing dot-stuffing.
async fn read_data<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> String {
    let mut data = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
            return data;
        }
        if line == ".\r\n" {
            return data;
        }
        data.push_str(
            line.strip_prefix('.')
                .filter(|_| line.starts_with(".."))
                .unwrap_or(&line),
        );
    }
}
