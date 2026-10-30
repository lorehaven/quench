---
title: Internals
parent: quench-mail
grand_parent: Crates
nav_order: 5
---

## Message wire format

`Message::formatted()` produces, in this order (CRLF after every line):

```text
Date: <RFC 2822 date>
From: <mailbox>
Reply-To: <mailboxes>        (if any)
To: <mailboxes>              (if any)
Cc: <mailboxes>              (if any)
Subject: <text>              (only if non-empty)
Message-ID: <uuid@domain>
MIME-Version: 1.0
<custom headers, in order added>
<body>
```

Each header goes through `mime::fold_header` (78 columns). Mailbox lists join with `, `; a mailbox with a name is `<phrase> <addr>`. The `Date` comes from `quench_core::time::Time::to_rfc2822()`.

Body, by which of text/html are set:

* Text only: `Content-Type: text/plain; charset=utf-8`, `Content-Transfer-Encoding: 7bit|quoted-printable`, blank line, body.
* HTML only: the same with `text/html`.
* Both: `Content-Type: multipart/alternative;\r\n boundary="quench-<32 hex>"`, blank line, the line `This is a multi-part message in MIME format.`, then for `text/plain` and `text/html` in that order a `--boundary` line, the part headers, a blank line and the body, and a closing `--boundary--`.

Transfer encoding is chosen per part: `7bit` if the CRLF-normalised body is ASCII, has no NUL and no line over 998 bytes (`mime::is_7bit`), else `quoted-printable`. The encoded body always ends with exactly one CRLF (one is appended if missing).

Custom headers are written with `mime::unstructured`; note that the value is also folded by `fold_header`, which collapses runs of spaces to a single space.

## SMTP dialogue

One TCP connection per `send`/`send_raw`/`test_connection`:

1. `TcpStream::connect((host, port))` under the timeout. For `Security::Tls` the TLS handshake follows immediately (server name = `tls_server_name` or host).
2. Read greeting, expect `220` (`stage "greeting"`).
3. `EHLO <hello_name>`. If the reply is not 250, `HELO <hello_name>` is tried and capabilities are cleared (so STARTTLS and AUTH are unavailable). Capabilities parsed from the extension lines: `STARTTLS`, `AUTH <mechanisms>` (upper-cased), `SIZE <n>`.
4. `Security::StartTls`: require the STARTTLS capability, send `STARTTLS`, expect `220`, require that no bytes are already buffered (else `Protocol`), upgrade the socket, then `EHLO` again.
5. If credentials are set: refuse when the session is not secure and the security mode is not `None`; use `AUTH PLAIN <base64(\0user\0pass)>` if `PLAIN` is offered, else `AUTH LOGIN` (expect 334, send base64 user, expect 334, send base64 password, expect 235); otherwise `Unsupported`.
6. Delivery: reject an empty recipient list (`Config("no recipients")`), check `SIZE`, `MAIL FROM:<addr>` (250), `RCPT TO:<addr>` for each (250 or 251), `DATA` (354), the dot-stuffed data, then expect 250.
7. `QUIT`, wait up to 2 s for a reply, shut the socket down; errors ignored.

`MAIL FROM` is sent without `SIZE=` or `BODY=` parameters.

## Security properties

* Certificates are verified against `webpki-roots` plus added PEM roots; no bypass.
* STARTTLS is never silently skipped, and bytes injected after the `220` response are rejected.
* Credentials are not sent over plaintext unless `allow_plaintext_auth(true)` and `Security::None`.
* `Credentials` has a `Debug` that prints `password: "<redacted>"`; `Mailer`'s `Debug` prints host, port, security only.
* CR/LF/NUL injection in subject, header values and names fails the build; addresses cannot contain characters that would break a command or header.
* Reply reading is bounded (4096-byte line, 100 lines) and every wait has a timeout.

## Limits

* No pipelining, no `8BITMIME`, no `SMTPUTF8`, no `AUTH XOAUTH2`/`CRAM-MD5`, no retries inside the crate, no pooling.
* A `Mailer` is stateless apart from configuration; it holds no connection.
