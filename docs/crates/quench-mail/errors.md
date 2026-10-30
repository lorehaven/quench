---
title: Errors
parent: quench-mail
grand_parent: Crates
nav_order: 4
---

Building a message and delivering it fail with different types. Both implement `std::error::Error` and `Display` through the `quench_core::Error` derive; the `Display` texts below are the `#[error(...)]` strings.

## `BuildError`

Returned by `MessageBuilder::build` and by `Address`/`Mailbox` parsing. Nothing has touched the network. Derives `Debug, Clone, PartialEq, Eq`.

| Variant | Display | When |
|---|---|---|
| `InvalidAddress(String)` | `invalid email address {0:?}` | An address or mailbox string failed validation (see [API](api.md)). The payload is the offending input. |
| `MissingFrom` | `message has no From address` | `build()` without `from`/`from_mailbox`, or `from` failed. |
| `NoRecipients` | `message has no recipients` | To, Cc and Bcc are all empty. |
| `MissingBody` | `message has neither a text nor an html body` | Neither `text` nor `html` set. |
| `HeaderInjection(&'static str)` | `{0} contains a line break or control character` | Payload is `"subject"`, `"header value"` or `"display name"`. Subject and header value: CR, LF or NUL. Display name: any control character. |
| `InvalidHeaderName(String)` | `invalid header name {0:?}` | Empty name, or a char that is not ASCII-graphic, or `:`. |
| `ReservedHeader(String)` | `header {0:?} is set by quench-mail itself and cannot be overridden` | Custom header whose lower-cased name is in the reserved list. |

## `Reply`

```rust
pub struct Reply { pub code: u16, pub enhanced: Option<String>, pub lines: Vec<String> }
```

`enhanced` is the RFC 3463 status (`5.7.1`) when the first line of the reply starts with a token of the form `d.d.d` with first digit 2, 4 or 5 and the other parts of 1-3 digits. That token is removed from every line that starts with it, and the lines are trimmed. `Display`: `<code>`, then ` <enhanced>` if present, then ` <lines joined by a space>`.

## `Error`

Returned by delivery and by `MailerBuilder`. Derives `Debug` only (it holds an `io::Error`).

| Variant | Display | When |
|---|---|---|
| `Io(io::Error)` | `network error: {0}` | Socket error (connection refused, reset, write failure). `From<io::Error>`. |
| `Tls(String)` | `TLS error: {0}` | Handshake failure (untrusted certificate, name mismatch) or TLS client config failure at `build()`. |
| `Timeout(&'static str)` | `timed out waiting for {0}` | Payload: `"the connection"`, `"the TLS handshake"`, `"the server to accept data"`, `"the server's reply"`. |
| `Protocol(String)` | `protocol error: {0}` | Not SMTP or too much of it: connection closed, reply not UTF-8, line too long, too many lines, malformed reply line, reply code changed mid-reply, data before TLS handshake after STARTTLS, cannot upgrade. |
| `Rejected { stage, reply }` | `server rejected {stage}: {reply}` | A reply with the wrong code. `stage` is one of `"greeting"` (not 220), `"HELO"`, `"STARTTLS"`, `"MAIL FROM"`, `"RCPT TO"` (anything but 250/251), `"DATA"` (not 354), `"end of DATA"` (not 250). |
| `Auth(Reply)` | `authentication failed: {0}` | `AUTH PLAIN` answered other than 235, or an `AUTH LOGIN` step other than 334/235. |
| `TooLarge { size, limit }` | `message is {size} bytes, the server accepts at most {limit}` | Data longer than the advertised `SIZE`. Checked before `MAIL FROM`. |
| `Unsupported(String)` | `{0}` | `"the server does not offer STARTTLS"`; credentials over a non-secure session; no `PLAIN` or `LOGIN` offered (`the server offers no supported login mechanism (offers: ...)`). |
| `Config(String)` | `invalid configuration: {0}` | Builder validation, or `"no recipients"` from `send_raw` with an empty envelope. |

### Methods

```rust
#[must_use] pub fn is_transient(&self) -> bool;
#[must_use] pub const fn reply(&self) -> Option<&Reply>;
```

* `is_transient()`: true for `Io`, `Timeout`, and `Rejected` with a code in `400..500`. False for everything else, including `Auth` with a 4xx code and `Tls`.
* `reply()`: `Some` for `Rejected` and `Auth`, `None` otherwise.
