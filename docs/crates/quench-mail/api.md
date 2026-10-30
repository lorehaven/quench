---
title: API reference
parent: quench-mail
grand_parent: Crates
nav_order: 2
---

Every public item of `quench_mail`, in source order (`src/lib.rs` re-exports). There is no `unsafe` code in the crate and no feature gate. `Mailer`, `MailerBuilder` and `Message` are `Send`; `Mailer` is also `Sync` (it holds an `Arc`).

## Crate root (`src/lib.rs`)

```rust
pub mod mime;
pub use address::{Address, Mailbox};
pub use error::{BuildError, Error, Reply};
pub use message::{Envelope, Message, MessageBuilder};
pub use smtp::{Mailer, MailerBuilder, Response, Security};
pub use smtp::dot_stuff; // #[doc(hidden)]
```

## `address` (`src/address.rs`)

Addresses are deliberately stricter than RFC 5322: ASCII only, no quoted local parts, no comments, no domain literals.

### `Address`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address { /* private: full: String, at: usize */ }

impl Address {
    pub fn new(local: &str, domain: &str) -> Result<Self, BuildError>;
    #[must_use] pub fn as_str(&self) -> &str;
    #[must_use] pub fn local(&self) -> &str;
    #[must_use] pub fn domain(&self) -> &str;
}
impl fmt::Display for Address {}            // writes as_str()
impl FromStr for Address { type Err = BuildError; }
```

* `new` formats `"{local}@{domain}"` and parses it.
* `FromStr` splits at the **last** `@` (`rsplit_once`). Errors with `BuildError::InvalidAddress(<the input>)` when there is no `@`, when the whole string is longer than 254 bytes, or when the local part or domain is invalid.
* Local part valid when: non-empty, at most 64 bytes, does not start or end with `.`, contains no `..`, and every char is ASCII alphanumeric, `.` or one of `` !#$%&'*+/=?^_`{|}~- ``.
* Domain valid when: non-empty, at most 253 bytes, and every `.`-separated label is non-empty, at most 63 bytes, neither starts nor ends with `-`, and consists of ASCII alphanumerics and `-`. A domain without a dot (`localhost`) is accepted; a trailing dot yields an empty label and is rejected.
* `local()` is the part before the `@` that was split off, `domain()` the part after it.

```rust
let a: quench_mail::Address = "alice@example.org".parse()?;
assert_eq!(a.local(), "alice");
assert!("a b@example.org".parse::<quench_mail::Address>().is_err());
```

### `Mailbox`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mailbox {
    pub name: Option<String>,
    pub address: Address,
}

impl Mailbox {
    #[must_use] pub const fn new(name: Option<String>, address: Address) -> Self;
}
impl From<Address> for Mailbox {}            // name: None
impl fmt::Display for Mailbox {}             // "Name <addr>" or "addr"; unencoded, for logs
impl FromStr for Mailbox { type Err = BuildError; }
```

`FromStr` trims the input and accepts `user@host`, `<user@host>`, `Name <user@host>` and `"Quoted, Name" <user@host>`:

* No `<`: the whole string is parsed as an `Address`.
* With a `<`: the **last** `<` is used; the text after it must end with `>` (otherwise `InvalidAddress(<input>)`), the inside is trimmed and parsed as an `Address`.
* The text before `<` is trimmed and unquoted: if it starts with `"`, backslash escapes are resolved up to the closing quote, and anything after the closing quote that is not whitespace, or a missing closing quote, gives `InvalidAddress`. An unquoted name passes through as written.
* A name containing a control character gives `BuildError::HeaderInjection("display name")`.
* An empty name becomes `None`.

`Mailbox::new` does not validate the name; the wire encoding is done by `Message::formatted` (see [Internals](internals.md)). This is how a display name from configuration is used verbatim.

## `error` (`src/error.rs`)

Full description in [Errors](errors.md).

```rust
#[derive(Debug, Clone, PartialEq, Eq, quench_core::Error)]
pub enum BuildError {
    InvalidAddress(String),
    MissingFrom,
    NoRecipients,
    MissingBody,
    HeaderInjection(&'static str),
    InvalidHeaderName(String),
    ReservedHeader(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub code: u16,
    pub enhanced: Option<String>,
    pub lines: Vec<String>,
}
impl Reply { #[must_use] pub fn message(&self) -> String; } // lines joined with a space
impl fmt::Display for Reply {}   // "<code>[ <enhanced>] <message>"

#[derive(Debug, quench_core::Error)]
pub enum Error {
    Io(io::Error),                                   // #[from]
    Tls(String),
    Timeout(&'static str),
    Protocol(String),
    Rejected { stage: &'static str, reply: Reply },
    Auth(Reply),
    TooLarge { size: usize, limit: usize },
    Unsupported(String),
    Config(String),
}
impl Error {
    #[must_use] pub fn is_transient(&self) -> bool;
    #[must_use] pub const fn reply(&self) -> Option<&Reply>;
}
```

## `message` (`src/message.rs`)

### `Envelope`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: Address,      // MAIL FROM
    pub to: Vec<Address>,   // one RCPT TO each
}
```

What the server is told, independent of the headers. A Bcc recipient is in `to` but in no header.

### `Message`

```rust
#[derive(Debug, Clone)]
pub struct Message { /* private fields */ }

impl Message {
    #[must_use] pub fn builder() -> MessageBuilder;
    #[must_use] pub fn message_id(&self) -> &str;
    #[must_use] pub fn envelope(&self) -> Envelope;
    #[must_use] pub fn formatted(&self) -> Vec<u8>;
}
```

* `builder()` returns `MessageBuilder::default()`.
* `message_id()` returns `<id@domain>` including the angle brackets: `<` + 32 hex chars of a random UUID v4 (simple form) + `@` + the domain of the From address + `>`.
* `envelope()`: `from` is the address given to `envelope_from` or else the From address. `to` is To, then Cc, then Bcc addresses in order, with duplicates removed (case-insensitive comparison of the whole address string; first occurrence wins).
* `formatted()`: the wire form, CRLF line endings, 7-bit clean. Header order: `Date`, `From`, `Reply-To` (if any), `To` (if any), `Cc` (if any), `Subject` (only if non-empty), `Message-ID`, `MIME-Version: 1.0`, then the custom headers in the order added, then the body part(s). Bcc is never written. Headers are folded at 78 columns. Details in [Internals](internals.md).

### `MessageBuilder`

```rust
#[derive(Debug, Default)]
pub struct MessageBuilder { /* private */ }

impl MessageBuilder {
    #[must_use] pub fn from(self, mailbox: &str) -> Self;
    #[must_use] pub fn from_mailbox(self, mailbox: Mailbox) -> Self;
    #[must_use] pub fn to_mailbox(self, mailbox: Mailbox) -> Self;
    #[must_use] pub fn to(self, mailbox: &str) -> Self;
    #[must_use] pub fn cc(self, mailbox: &str) -> Self;
    #[must_use] pub fn bcc(self, mailbox: &str) -> Self;
    #[must_use] pub fn reply_to(self, mailbox: &str) -> Self;
    #[must_use] pub fn subject(self, subject: &str) -> Self;
    #[must_use] pub fn text(self, body: &str) -> Self;
    #[must_use] pub fn html(self, body: &str) -> Self;
    #[must_use] pub fn header(self, name: &str, value: &str) -> Self;
    #[must_use] pub const fn date(self, date: quench_core::time::Time) -> Self;
    #[must_use] pub fn envelope_from(self, address: &str) -> Self;
    pub fn build(self) -> Result<Message, BuildError>;
}
```

Error handling model: no setter returns a `Result`. The first problem is stored and `build()` returns it; later problems are ignored (`get_or_insert`).

| Method | Behaviour |
|---|---|
| `from(&str)` | Parses a `Mailbox`. Sets From (replaces any earlier From). On a parse error From is set to `None` and the error is stored. |
| `from_mailbox(Mailbox)` | Sets From without parsing. |
| `to_mailbox(Mailbox)` | Appends a To recipient without parsing. There are no `cc_mailbox`/`bcc_mailbox` variants. |
| `to`, `cc`, `bcc`, `reply_to` | Parse a `Mailbox` and append to that list; calls accumulate. `bcc` recipients get a `RCPT TO` but are named in no header. |
| `subject(&str)` | Stored; a CR, LF or NUL fails with `HeaderInjection("subject")` (the previous subject is kept). Default: empty (no `Subject` header is written). |
| `text(&str)` / `html(&str)` | Set the body part. Both together give `multipart/alternative`. Calling again replaces. |
| `header(name, value)` | Adds a custom header. `name` must be non-empty and made of ASCII graphic characters except `:`, else `InvalidHeaderName(name)`. A case-insensitive match against the reserved list gives `ReservedHeader(name)`. A CR, LF or NUL in the value gives `HeaderInjection("header value")`. The value is written through `mime::unstructured` (encoded words if non-ASCII). |
| `date(Time)` | Overrides the `Date` header; default `Time::now()` at `build()`. |
| `envelope_from(&str)` | Parses an `Address` (not a mailbox) for `MAIL FROM`; parse error stored. |
| `build()` | Returns the stored error if any; else `MissingFrom` if no From; `NoRecipients` if To, Cc and Bcc are all empty (Reply-To does not count); `MissingBody` if neither text nor html is set. Generates the Message-ID. |

Reserved header names (case-insensitive): `from`, `to`, `cc`, `bcc`, `reply-to`, `sender`, `subject`, `date`, `message-id`, `mime-version`, `content-type`, `content-transfer-encoding`.

## `smtp` (`src/smtp.rs`)

### `Security`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Tls,       // TLS from the first byte; default port 465
    StartTls,  // plain, then STARTTLS; default port 587
    None,      // no encryption; default port 25
}
```

### `Response`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response { pub reply: Reply }
impl Response { #[must_use] pub fn message(&self) -> String; } // reply.message()
```

The server's `250` reply to the end of DATA. `message()` has the enhanced status code removed (it is in `reply.enhanced`).

### `Mailer`

```rust
#[derive(Clone)]
pub struct Mailer { /* Arc<Inner> */ }   // Debug prints host, port, security only

impl Mailer {
    #[must_use] pub fn builder(host: &str) -> MailerBuilder;
    pub async fn send(&self, message: &Message) -> Result<Response, Error>;
    pub async fn send_raw(&self, envelope: &Envelope, data: &[u8]) -> Result<Response, Error>;
    pub async fn test_connection(&self) -> Result<(), Error>;
}
```

* `builder(host)` starts with `Security::Tls`, no port override, no credentials, EHLO name `localhost`, timeout 30 s, no extra roots, plaintext auth off.
* `send` = `send_raw(&message.envelope(), &message.formatted())`.
* `send_raw` opens a new connection, runs the dialogue ([Internals](internals.md)), always sends `QUIT` and returns the result of the delivery. `data` must use CRLF; it is dot-stuffed and terminated by the library.
* `test_connection` connects, secures and logs in, then quits. A mailer without credentials only connects and greets.
* Errors: see [Errors](errors.md).

### `MailerBuilder`

```rust
#[derive(Debug)]
pub struct MailerBuilder { /* private */ }

impl MailerBuilder {
    #[must_use] pub const fn security(self, security: Security) -> Self;
    #[must_use] pub const fn port(self, port: u16) -> Self;
    #[must_use] pub fn tls_server_name(self, name: &str) -> Self;
    #[must_use] pub fn credentials(self, username: &str, password: &str) -> Self;
    #[must_use] pub fn hello_name(self, name: &str) -> Self;
    #[must_use] pub const fn timeout(self, timeout: Duration) -> Self;
    pub fn add_root_certificate_pem(self, pem: &str) -> Result<Self, Error>;
    #[must_use] pub const fn allow_plaintext_auth(self, allow: bool) -> Self;
    pub fn build(self) -> Result<Mailer, Error>;
}
```

All options and defaults are in [Configuration](configuration.md). `MailerBuilder` is only obtainable from `Mailer::builder`. Its `Debug` output prints the credentials with `password: "<redacted>"`.

### `dot_stuff`

```rust
#[doc(hidden)]
#[must_use]
pub fn dot_stuff(data: &[u8]) -> Vec<u8>;
```

Doubles a `.` at the start of any line, appends `\r\n` if the data does not end with it, then appends `.\r\n`. Hidden from rustdoc but exported (used by the tests).

## `mime` (`src/mime.rs`, `pub mod mime`)

Pure functions over text, no I/O. All are `#[must_use]`.

```rust
pub fn normalize_crlf(text: &str) -> String;
pub fn is_plain_header_text(text: &str) -> bool;
pub fn encoded_words(text: &str) -> Vec<String>;
pub fn unstructured(text: &str) -> String;
pub fn phrase(name: &str) -> String;
pub fn fold_header(name: &str, value: &str) -> String;
pub fn is_7bit(text: &str) -> bool;
pub fn quoted_printable(text: &str) -> String;
```

| Function | Behaviour |
|---|---|
| `normalize_crlf` | Replaces `\n`, `\r\n` and a lone `\r` each with `\r\n`. |
| `is_plain_header_text` | True when every char is tab or in `' '..='~'` (printable ASCII and space). |
| `encoded_words` | Splits `text` into chunks of at most 45 bytes on char boundaries; each becomes `=?UTF-8?B?<base64>?=`. Empty input gives an empty vector. |
| `unstructured` | The text unchanged if plain, else the encoded words joined with a single space. |
| `phrase` | Non-plain: encoded words joined by a space. Plain with any of `()<>[]:;@\,."`: a quoted string with `"` and `\` backslash-escaped. Otherwise unchanged. |
| `fold_header` | `Name:` followed by the value's space-separated tokens (empty tokens dropped, so runs of spaces collapse). Breaks with `\r\n ` before a token when it would pass 78 columns and the line already holds a token. A token longer than the limit stays whole. Does not append a trailing CRLF. |
| `is_7bit` | True for text that is ASCII, has no NUL, and no `\r\n`-separated line over 998 bytes. Input must already be CRLF-normalised. |
| `quoted_printable` | RFC 2045 quoted-printable of CRLF-normalised text: lines of at most 76 columns with `=\r\n` soft breaks; `=XX` (uppercase hex) for bytes outside `33..=60` and `62..=126` (so `=` is escaped); a space or tab at the end of a line is escaped. Space and tab inside a line are literal. Hard line breaks (`\r\n`) are kept. |

```rust
use quench_mail::mime;
assert_eq!(mime::normalize_crlf("a\nb\rc\r\nd"), "a\r\nb\r\nc\r\nd");
assert_eq!(mime::phrase("Doe, Jane"), "\"Doe, Jane\"");
assert!(mime::unstructured("Zażółć").starts_with("=?UTF-8?B?"));
```
