# quench-mail
Email: a MIME message builder and an async SMTP submission client.

## What it is
`quench-mail` builds an email message and submits it to an authenticated SMTP server over TLS or STARTTLS. It exists so a service can send a verification or password-reset mail without a general-purpose mail stack: compose a message, then deliver it.

Out of scope: attachments, receiving mail and connection pooling. It builds on `quench-core` (base64, time, uuid) and `quench-tls` for the encrypted connection, and runs on `tokio`.

## Use cases
- Send address-confirmation or password-reset mails from a service.
- Compose multipart text and HTML messages with non-ASCII subjects and names.
- Send to Bcc recipients and set a separate bounce address.
- Check a mail server connection at startup.

## Example
```rust
use quench_mail::{Mailer, Message, Security};

let mailer = Mailer::builder("mail.example.com")
    .security(Security::Tls)
    .credentials("noreply@example.com", "app-password")
    .build()?;

let message = Message::builder()
    .from("Forge <noreply@example.com>")
    .to("alice@example.org")
    .subject("Confirm your address")
    .text("Follow this link: https://example.com/verify?token=abc")
    .html("<p>Follow this link to confirm.</p>")
    .build()?;

mailer.send(&message).await?;
```

With both `.text` and `.html` the message is `multipart/alternative`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-mail/
