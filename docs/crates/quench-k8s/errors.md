---
title: Errors
parent: quench-k8s
grand_parent: Crates
nav_order: 4
---

```rust
pub type Result<T> = std::result::Result<T, Error>;
```

## `Status`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status { pub code: u16, pub reason: String, pub message: String }
```

The API server's account of a failure. `Display`: `"{code} {reason}: {message}"`.

Built from the response body: if it parses as JSON with `kind == "Status"`, `code` is the body's `code` (falling back to the HTTP status), `reason` and `message` are the body's strings (empty if absent). For any other body, `code` is the HTTP status, `reason` is empty, `message` is the trimmed body text.

## `Error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Config(String),
    Transport(String),
    Api(Status),
    Decode(String),
    Unsupported(String),
}
impl std::error::Error for Error {}
```

| Variant | `Display` | When |
|---|---|---|
| `Config(m)` | `kubernetes configuration: {m}` | Kubeconfig or service account missing, unreadable or incomplete; HTTP client could not be built; unreadable token file; a namespaced `Api` without a namespace used for a per-object call. |
| `Transport(m)` | `kubernetes request failed: {m}` | Server unreachable, connection broke, timeout, or reading the body failed. |
| `Api(status)` | `kubernetes API error: {status}` | Any non-success HTTP status. |
| `Decode(m)` | `kubernetes response: {m}` | A body that is not JSON, a watch line that is not JSON, a watch event without `object`, an unknown watch event type (`unknown watch event type <Option<&str> debug>`). |
| `Unsupported(m)` | `not supported: {m}` | Credential plugins in a kubeconfig. |

### Methods

```rust
#[must_use] pub const fn status(&self) -> Option<&Status>;   // Some for Error::Api
#[must_use] pub fn is_not_found(&self) -> bool;              // Api with code 404
#[must_use] pub fn is_conflict(&self) -> bool;               // Api with code 409
#[must_use] pub fn is_forbidden(&self) -> bool;              // Api with code 403
```

409 is both "already exists" (on `create`) and "the update raced another" (on `replace`).
