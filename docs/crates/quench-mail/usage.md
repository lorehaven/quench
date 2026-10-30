---
title: Usage
parent: quench-mail
grand_parent: Crates
nav_order: 1
---

Task-oriented examples. All code uses names that exist in `quench_mail`.

## 1. Send a plain-text mail over implicit TLS

Goal: one message through a submission server on port 465.

```rust
use quench_mail::{Mailer, Message, Security};

async fn send(password: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mailer = Mailer::builder("mail.example.com")
        .security(Security::Tls) // the default; port 465
        .credentials("noreply@example.com", password)
        .build()?;
    let message = Message::builder()
        .from("Forge <noreply@example.com>")
        .to("alice@example.org")
        .subject("Confirm your address")
        .text("https://example.com/verify?token=abc")
        .build()?;
    let response = mailer.send(&message).await?;
    println!("accepted: {}", response.message());
    Ok(())
}
```

Effect: connects, TLS handshake, `EHLO`, `AUTH`, `MAIL FROM`, `RCPT TO`, `DATA`, `QUIT`. `response.message()` is the text of the final `250` reply with the leading enhanced status (`2.0.0`) removed, for example `queued as TEST1` (the code is in `response.reply.enhanced`).

Pitfalls: the mailer opens one connection per `send`. `Mailer` is cheap to `Clone`.

## 2. Text and HTML together

```rust
# use quench_mail::Message;
let message = Message::builder()
    .from("noreply@example.com")
    .to("alice@example.org")
    .subject("Welcome")
    .text("Plain version")
    .html("<p>HTML version</p>")
    .build()?;
# Ok::<(), quench_mail::BuildError>(())
```

With both bodies the message is `multipart/alternative` (plain part first, HTML second). With one body it is a single `text/plain` or `text/html` part. With none, `build()` returns `BuildError::MissingBody`.

## 3. Names, non-ASCII subjects, extra headers

```rust
# use quench_mail::Message;
let message = Message::builder()
    .from(r#""Doe, Jane" <jane@example.com>"#)
    .to("Zoë <zoe@example.org>")
    .reply_to("support@example.com")
    .subject("Zażółć gęślą jaźń")
    .text("Treść")
    .header("Auto-Submitted", "auto-generated")
    .build()?;
# Ok::<(), quench_mail::BuildError>(())
```

The subject and the name `Zoë` are written as RFC 2047 UTF-8 base64 encoded words; `"Doe, Jane"` is written as a quoted string. A non-ASCII body is sent `quoted-printable`. Setting `.header("Subject", ..)` or any other reserved header fails the build with `BuildError::ReservedHeader`.

## 4. Bcc and a bounce address

```rust
# use quench_mail::Message;
let message = Message::builder()
    .from("noreply@example.com")
    .to("alice@example.org")
    .bcc("audit@example.com")
    .envelope_from("bounces@example.com")
    .subject("Report")
    .text("...")
    .build()?;
let envelope = message.envelope();
assert_eq!(envelope.from.as_str(), "bounces@example.com");
assert_eq!(envelope.to.len(), 2); // alice and audit; audit is in no header
# Ok::<(), quench_mail::BuildError>(())
```

## 5. Retry only what can succeed later

```rust
# use quench_mail::{Mailer, Message};
# async fn go(mailer: Mailer, message: Message) {
match mailer.send(&message).await {
    Ok(response) => println!("accepted: {}", response.message()),
    Err(err) if err.is_transient() => { /* queue and retry later */ }
    Err(err) => {
        eprintln!("refused: {err}");
        if let Some(reply) = err.reply() {
            eprintln!("code {} enhanced {:?}", reply.code, reply.enhanced);
        }
    }
}
# }
```

`is_transient()` is true for `Error::Io`, `Error::Timeout` and `Error::Rejected` with a 4xx code only.

## 6. STARTTLS on port 587 and an in-cluster host name

```rust
use quench_mail::{Mailer, Security};
let mailer = Mailer::builder("smtp.mail.svc.cluster.local")
    .security(Security::StartTls)        // port 587
    .tls_server_name("mail.example.com") // the name the certificate is valid for
    .credentials("noreply@example.com", "secret")
    .hello_name("forge.example.com")
    .build()?;
# Ok::<(), quench_mail::Error>(())
```

With `StartTls`, if the server does not advertise `STARTTLS` the send fails with `Error::Unsupported("the server does not offer STARTTLS")`; it never falls back to plaintext.

## 7. A private CA

```rust
use quench_mail::{Mailer, Security};
let pem = std::fs::read_to_string("ca.pem")?;
let mailer = Mailer::builder("mail.internal")
    .security(Security::Tls)
    .add_root_certificate_pem(&pem)?  // Result<MailerBuilder, Error>
    .build()?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

There is no switch that disables certificate verification.

## 8. Startup check

```rust
# async fn check(mailer: quench_mail::Mailer) -> Result<(), quench_mail::Error> {
mailer.test_connection().await?; // connect, secure, log in, QUIT; no mail is sent
# Ok(()) }
```

## 9. Unauthenticated local relay

```rust
use quench_mail::{Mailer, Security};
let mailer = Mailer::builder("127.0.0.1")
    .security(Security::None) // port 25
    .build()?;
# Ok::<(), quench_mail::Error>(())
```

Adding `.credentials(..)` to a `Security::None` builder makes `build()` fail unless `.allow_plaintext_auth(true)` is also set.

## 10. Send pre-formatted bytes

`Mailer::send_raw(&Envelope, &[u8])` sends bytes you produced yourself (CRLF line endings expected). Build the `Envelope` with its public fields `from` and `to`. Dot-stuffing and the final `.` are added for you.
