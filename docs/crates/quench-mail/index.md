---
title: quench-mail
parent: Crates
nav_order: 31
has_children: true
permalink: /crates/quench-mail/
---

`quench-mail` (library `quench_mail`) builds an email message and submits it to an authenticated SMTP server over TLS or STARTTLS. It exists so a service can send a verification or password-reset mail without a general-purpose mail stack. It does two things: compose a message, and deliver it.

* Version: `0.1.12` (`quench-mail/Cargo.toml`). Edition, authors, license and repository come from the workspace.
* Not in scope (stated in `src/lib.rs`): attachments, receiving mail, connection pooling. Also absent from the code: DKIM signing, SMTPUTF8 (addresses are ASCII only).

## Install

```toml
[dependencies]
quench-mail = "0.1"
```

The crate has no `[features]` table.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `Error` derive macro (`quench_core::Error`), `base64::STANDARD`, `time::Time` (the `Date` header), `uuid::Uuid` (Message-ID and MIME boundary). |
| `quench-tls` | `ClientOptions`, `client_config`, `root_store`, `server_name`, `TlsConnector`, `TlsStream`, `ServerName` for TLS and STARTTLS. |
| `tokio` | Sockets (`TcpStream`), I/O traits, timeouts. |

Dev-dependencies: `quench-test`, `tokio`. No third-party crates other than what the workspace `tokio` brings.

## Minimal example

From the crate docs in `src/lib.rs`:

```rust
use quench_mail::{Mailer, Message, Security};

# async fn demo() -> Result<(), Box<dyn std::error::Error>> {
let mailer = Mailer::builder("mail.example.com")
    .security(Security::Tls)
    .credentials("noreply@example.com", "app-password")
    .build()?;

let message = Message::builder()
    .from("Forge <noreply@example.com>")
    .to("alice@example.org")
    .subject("Confirm your address")
    .text("Follow this link to confirm your address: https://example.com/verify?token=abc")
    .build()?;

mailer.send(&message).await?;
# Ok(())
# }
```

## Where to go next

* [Usage](usage.md): worked examples (HTML + text, Bcc, retry on transient errors, STARTTLS, private CA, startup check).
* [API reference](api.md): every public item with its signature.
* [Configuration](configuration.md): builder options, defaults and limits.
* [Errors](errors.md): `BuildError`, `Error`, `Reply`.
* [Internals](internals.md): wire format, SMTP dialogue, security properties.
* [Testing](testing.md): the in-process SMTP server used by the tests.

The crate public items are re-exported at the crate root: `Address`, `Mailbox`, `BuildError`, `Error`, `Reply`, `Envelope`, `Message`, `MessageBuilder`, `Mailer`, `MailerBuilder`, `Response`, `Security`, `dot_stuff` (hidden from rustdoc), and the module `mime`.
