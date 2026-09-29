# Quench Mail

`quench-mail` (crate `quench_mail`) builds an email and submits it to an SMTP server. It exists so a service can send a verification or password-reset link without pulling in a general-purpose mail stack: it does two things, both small - compose a message, and deliver it to an authenticated submission server (port 465 or 587) - and does them strictly.

Not in scope: attachments, receiving or reading mail, connection pooling, DKIM signing (the submission server signs), SMTPUTF8 (addresses are ASCII; send an internationalised domain as punycode).

## Public API / Key Types

- **`Message`** / **`MessageBuilder`** - `Message::builder().from("Name <a@b.c>").to(..).cc(..).bcc(..).reply_to(..).subject(..).text(..).html(..).header(name, value).envelope_from(..).build()`.
  - `from_mailbox` / `to_mailbox` take an already-built `Mailbox` (a display name from configuration is used verbatim, never re-parsed).
  - Address arguments are parsed lazily: the first bad one is reported by `build()`, so calls chain without `?`.
  - `text` and `html` together produce `multipart/alternative` (plain first, HTML preferred); either alone is a single part.
  - Bodies are `7bit` when they are ASCII with short lines, `quoted-printable` otherwise; line endings are normalised to CRLF. Non-ASCII subjects, custom header values and display names become RFC 2047 encoded words; display names with specials are quoted.
  - `Date` (now) and `Message-ID` (`<uuid@sender-domain>`) are generated; headers are folded at 78 columns.
  - `Message::formatted()` is the wire form (7-bit clean, CRLF only); `Message::envelope()` is what the server is told - `MAIL FROM` is the `From` address unless `envelope_from` says otherwise, recipients are To + Cc + Bcc without duplicates. Bcc appears in no header.
- **`Address`** / **`Mailbox`** - validated addresses (`FromStr`). Deliberately stricter than RFC 5322: ASCII only, no quoted local parts, comments or domain literals, so an accepted address is safe in both an SMTP command and a header.
- **`Mailer`** / **`MailerBuilder`** - `Mailer::builder(host)` then `.security(Security::Tls | StartTls | None)`, `.port(..)` (default 465 / 587 / 25 by security), `.credentials(user, password)`, `.tls_server_name(..)`, `.hello_name(..)`, `.timeout(..)` (default 30 s per step), `.add_root_certificate_pem(..)`, `.allow_plaintext_auth(..)`, `.build()`.
  - `send(&Message)` and `send_raw(&Envelope, &[u8])` return a `Response` (the server's `250` text, e.g. its queue id).
  - `test_connection()` connects, secures and logs in without sending - a startup check that the settings work.
  - One connection per message; `Mailer` is cheap to clone.
- **`Error`** / **`BuildError`** / **`Reply`** - building a message and delivering it fail differently. `Error::is_transient()` is true for network errors, timeouts and 4xx replies (worth retrying later) and false for a 5xx refusal, a bad login or a message problem. `Error::reply()` gives the server's code, RFC 3463 enhanced status and text.

## Security behaviour

- **TLS is verified.** Server certificates are checked against the built-in public roots (`webpki-roots`) plus any `add_root_certificate_pem`. There is no "accept invalid certificates" switch. `tls_server_name` lets the certificate be validated for a different name than the host connected to, e.g. connecting to an in-cluster service name while the certificate is for the public hostname.
- **No silent downgrade.** With `Security::StartTls` the server must offer STARTTLS, otherwise the send fails - it never continues in plaintext. Anything the server sends after its `220` to STARTTLS (a classic injection attack) is a protocol error.
- **Credentials need TLS.** `build()` refuses credentials with `Security::None` unless `allow_plaintext_auth(true)`. Login uses `AUTH PLAIN`, or `AUTH LOGIN` if PLAIN is not offered. `Debug` output never prints the password.
- **Injection is refused at the source.** CR, LF and NUL in a subject, header value or display name fail the build; custom headers cannot override the ones the library writes (From, Subject, Content-Type, ...).
- **Bounded reads.** Reply lines and multi-line replies have size caps, and every wait on the server has a timeout.

## Configuration

The crate reads no environment variables; the caller passes everything to `MailerBuilder`.

## Testing

`tests/unit/` holds address, MIME and message tests, and SMTP client tests that run against an in-process scriptable SMTP server (`tests/unit/support/`) over real sockets and real TLS, using the throwaway certificate in `tests/fixtures/` (valid until 2126, for `localhost`, never trusted outside the tests). They cover implicit TLS, STARTTLS (including that EHLO is asked again after the upgrade and login happens only after it), PLAIN and LOGIN, wrong passwords, temporary and permanent refusals, dot-stuffing, size limits, certificate and name mismatches, STARTTLS injection, timeouts and hang-ups.

## Usage example

```rust
use quench_mail::{Mailer, Message, Security};

let mailer = Mailer::builder("stalwart-smtp.stalwart.svc.cluster.local")
    .security(Security::Tls)
    .tls_server_name("mail.example.com")
    .credentials("noreply@example.com", &password)
    .build()?;

let message = Message::builder()
    .from("Forge <noreply@example.com>")
    .to("alice@example.org")
    .subject("Confirm your address")
    .text("Follow this link to confirm your address: https://example.com/verify?token=abc")
    .header("Auto-Submitted", "auto-generated")
    .build()?;

match mailer.send(&message).await {
    Ok(response) => tracing::info!("accepted: {}", response.message()),
    Err(err) if err.is_transient() => { /* try again later */ }
    Err(err) => tracing::error!("mail refused: {err}"),
}
```

[Home](../README.md)
