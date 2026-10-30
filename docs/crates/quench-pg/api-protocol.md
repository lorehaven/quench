---
title: API - wire protocol and SCRAM
parent: quench-pg
grand_parent: Crates
nav_order: 4
---

Two public modules for tools and tests that speak the protocol directly: `quench_pg::message` (`src/message.rs`) and `quench_pg::scram` (`src/scram.rs`). Ordinary users do not need them; `Connection` uses them.

## `message`: encoding client messages

Each function returns the complete frame as `Vec<u8>` (all `#[must_use]`). Tagged frames are `tag (1 byte), length (u32 big-endian, counts itself and the body), body`; the two untagged startup-phase frames are `length, body`. Integers are big-endian. Strings are NUL-terminated UTF-8.

| Function | Frame |
|---|---|
| `startup(params: &[(&str, &str)])` | untagged: protocol version `196608` (3.0), then each `name\0value\0`, then a final `0` byte. |
| `ssl_request()` | untagged: the code `80877103`. The server answers one byte, `S` or `N`. |
| `password(password: &str)` | `p` + the password string (cleartext authentication). |
| `sasl_initial_response(mechanism: &str, data: &[u8])` | `p` + mechanism string + `i32` length of `data` + `data`. |
| `sasl_response(data: &[u8])` | `p` + `data`. |
| `query(sql: &str)` | `Q` + the SQL string (simple protocol). |
| `parse(name: &str, sql: &str, param_types: &[u32])` | `P` + statement name + SQL + `i16` parameter count + one `u32` OID each (0 lets the server infer). |
| `bind(portal: &str, statement: &str, params: &[Option<String>])` | `B` + portal + statement + `i16 1, i16 0` (one parameter format code: text) + `i16` parameter count + per parameter `i32 -1` (NULL) or `i32` length + bytes + `i16 1, i16 0` (one result format code: text). |
| `describe(kind: u8, name: &str)` | `D` + `kind` (`b'S'` statement or `b'P'` portal) + name. |
| `execute(portal: &str, max_rows: u32)` | `E` + portal + `u32` max rows (0 = all). |
| `close(kind: u8, name: &str)` | `C` + `kind` + name. |
| `sync()` | `S` with an empty body. |
| `terminate()` | `X` with an empty body. |

Counts and lengths that overflow their field saturate (`i16::MAX`, `i32::MAX`, `u32::MAX`) rather than panic; `Connection` rejects more than `i16::MAX` parameters before building a frame.

## `message`: decoding server messages

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescription {
    pub name: String,
    pub table_oid: u32,
    pub column_id: i16,
    pub type_oid: u32,
    pub type_size: i16,
    pub type_modifier: i32,
    pub format: i16,            // 0 text, 1 binary
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    AuthenticationOk,                          // 'R', code 0
    AuthenticationCleartextPassword,           // code 3
    AuthenticationMd5Password([u8; 4]),        // code 5 (salt)
    AuthenticationSasl(Vec<String>),           // code 10 (mechanism names)
    AuthenticationSaslContinue(Vec<u8>),       // code 11
    AuthenticationSaslFinal(Vec<u8>),          // code 12
    AuthenticationOther(u32),                  // any other 'R' code (Kerberos, GSS, SSPI, ...)
    ParameterStatus { name: String, value: String },       // 'S'
    BackendKeyData { process_id: u32, secret_key: u32 },   // 'K'
    ReadyForQuery(u8),                         // 'Z': b'I', b'T' or b'E'
    RowDescription(Vec<FieldDescription>),     // 'T'
    DataRow(Vec<Option<Vec<u8>>>),             // 'D'; None is NULL
    CommandComplete(String),                   // 'C'
    EmptyQueryResponse,                        // 'I'
    ParseComplete,                             // '1'
    BindComplete,                              // '2'
    CloseComplete,                             // '3'
    NoData,                                    // 'n'
    PortalSuspended,                           // 's'
    ParameterDescription(Vec<u32>),            // 't'
    ErrorResponse(ServerError),                // 'E'
    NoticeResponse(ServerError),               // 'N'
    NotificationResponse { process_id: u32, channel: String, payload: String },  // 'A'
    Unknown(u8),                               // any other tag
}

pub fn decode(tag: u8, body: &[u8]) -> Result<Backend>
#[must_use] pub fn rows_affected(tag: &str) -> u64
```

* `decode`: decodes one message from its type byte and body (the length prefix is handled by the connection). `Error::Protocol("message shorter than its contents")`, `"unterminated string in message"` or `"negative column length {n}"` for malformed bodies. Strings are decoded lossily as UTF-8 (invalid bytes become U+FFFD). `ErrorResponse` and `NoticeResponse` are parsed with the field codes described under [`ServerError`](errors.md#struct-servererror).
* `rows_affected`: the number in the last space-separated word of a command tag (`INSERT 0 3` is 3, `UPDATE 2` is 2, `SELECT 5` is 5); 0 if there is none.

How `Connection` treats messages: `NoticeResponse` is logged at debug level (`postgres notice: ...`) and skipped; `ParameterStatus` updates the parameter map; `NotificationResponse` is logged (`postgres notification on `{channel}` ignored`) and skipped. A frame whose length field is below 4 or whose body exceeds `Config::max_message_size` closes the connection with `Error::Protocol("message length {len} is out of range")`.

## `scram`: SCRAM-SHA-256

RFC 5802 with RFC 7677's parameters, client side, as PostgreSQL speaks it. Channel binding is not used (the gs2 header is `n,,`); the user name in the exchange is empty (`n=`), PostgreSQL takes the user from the startup message.

```rust
pub const MECHANISM: &str = "SCRAM-SHA-256";

#[derive(Debug)]
pub struct Scram { /* password, client_nonce, client_first_bare, expected_server_signature */ }

#[must_use] pub fn new(password: &str) -> Self
#[must_use] pub fn with_nonce(password: &str, nonce: &str) -> Self
#[must_use] pub fn client_first(&self) -> Vec<u8>
pub fn client_final(&mut self, server_first: &[u8]) -> Result<Vec<u8>>
pub fn verify_server(&self, server_final: &[u8]) -> Result<()>
```

* `new`: a fresh exchange; the nonce is 18 random bytes (`quench_core::random::bytes::<18>`) base64-encoded, i.e. 24 characters.
* `with_nonce`: a chosen nonce, for tests against published vectors.
* `client_first`: `n,,n=,r={nonce}` as bytes.
* `client_final(server_first)`: parses `r=`, `s=`, `i=` (an `m=` attribute is an error: "the server requires an unsupported SCRAM extension"), checks the server nonce starts with ours and is longer, base64-decodes the salt, requires `1 <= iterations <= 5_000_000`, derives the salted password with PBKDF2-HMAC-SHA-256, and returns `c=biws,r={nonce},p={base64 proof}`; it remembers the server signature it expects. All failures are `Error::Auth`.
* `verify_server(server_final)`: `e=...` is `Error::Auth("the server rejected the SCRAM exchange: ...")`; `v=<base64>` is compared in constant time (`quench_crypto::ct::eq`) with the expected signature; a mismatch is `Error::Auth("the server's SCRAM signature does not match: it does not know the password")`. Calling it before `client_final` is an `Auth` error.

The password is used as raw UTF-8 bytes (no SASLprep normalisation).
