---
title: API - client
parent: quench-fullstack
grand_parent: Crates
nav_order: 3
---

Module `quench_fullstack::client` (`src/client.rs`), available on every target; it is meant for the browser (it uses `quench_ui_web::{Request, Response, FetchError, sleep}`), and also runs on the native host of `quench-ui-web` in tests. Futures hold `Rc`s and are not `Send` (`#![allow(clippy::future_not_send)]`). State is per-thread (`thread_local!`).

```rust
pub struct Tokens { pub access: String, pub refresh: String }       // Clone, Debug, PartialEq, Eq

pub trait TokenStore {
    fn load(&self) -> Option<Tokens>;
    fn save(&self, tokens: Option<Tokens>);
}

#[derive(Debug, Default)]
pub struct MemoryStore(/* RefCell<Option<Tokens>> */);
impl TokenStore for MemoryStore {}

#[derive(Clone)]
pub struct Bearer { /* base: String, store: Rc<dyn TokenStore> */ }
impl Bearer {
    #[must_use] pub fn new(base: &str, store: Rc<dyn TokenStore>) -> Self;
    #[must_use] pub fn signed_in(&self) -> bool;
    #[must_use] pub fn access_token(&self) -> Option<String>;
    pub async fn login(&self, username: &str, password: &str) -> Result<(), ServerError>;
    pub async fn refresh(&self) -> Result<(), ServerError>;
    pub async fn logout(&self);
}

pub fn use_bearer(bearer: Bearer);
pub fn on_unauthorized(handler: impl Fn() + 'static);
pub fn reset();
pub async fn call<A: quench_serde::Serialize, T: Deserialize>(path: &str, args: &A) -> Result<T, ServerError>;
pub async fn sign_in(base: &str, username: &str, password: &str) -> Result<(), ServerError>;
pub async fn sign_out(base: &str) -> Result<(), ServerError>;
```

## Tokens, `TokenStore`, `MemoryStore`

`TokenStore` is where a host keeps the tokens (the browser's own storage is not used; a shell implements it over its secure storage). `save(None)` forgets the tokens. `MemoryStore` keeps them in memory only.

## `Bearer`

Bearer-token sessions against a gatehouse-compatible endpoint. `Debug` prints `base` and `signed_in`.

| Method | Behaviour |
|---|---|
| `new(base, store)` | `base` is the auth API path or URL (`/api/v1/auth`); trailing `/` removed. |
| `signed_in()` | `store.load().is_some()`. |
| `access_token()` | The stored access token. |
| `login(user, pass)` | `POST {base}/login` with `{"username","password"}`; stores `access_token` and `refresh_token` from the answer. |
| `refresh()` | `POST {base}/refresh` with `{"refresh_token"}`; stores the new pair. No tokens: `Err(Unauthorized("not signed in"))`. If the result is `Unauthorized` the store is cleared. |
| `logout()` | If tokens exist, `POST {base}/logout` with `{"refresh_token"}` (errors ignored); then clears the store. |

All requests are `POST` with `content-type` and `accept: application/json` and `x-quench-rpc: 1`. A refused exchange maps via the answer's `error` field (underscores replaced by spaces; else `"sign-in refused ({status})"`): 401 and 403 to `Unauthorized`, 500 and above to `Internal`, any other status to `BadResponse`. An unparsable success body is `BadResponse("unreadable tokens: ...")`. Transport failure is `Network(message)`; an aborted request is `Network("request aborted")`.

## Free functions

| Function | Behaviour |
|---|---|
| `use_bearer(bearer)` | Every later `call` carries `Authorization: Bearer <access>` and refreshes on a 401. |
| `on_unauthorized(handler)` | Runs when a call ends as `Unauthorized` and no refresh can help (also on a plain 401/`unauthorized` body). Replaces any earlier handler. |
| `reset()` | Forgets the bearer, the handler, the refresh generation and any refresh in progress. For tests. |
| `call(path, args)` | What `#[server]` expands to in the browser. Serializes `args` to JSON, `POST`s it to `path` with the RPC header (and bearer if set). On 401 with a bearer set, renews once (`renewed` flag) and retries once; a failed renewal runs the handler and returns that error. Success: body decoded as `T` (`BadResponse("unreadable answer: ...")` on failure). Failure: `ServerError::from_response(status, body)`; `Unauthorized` runs the handler. |
| `sign_in(base, user, pass)` | Cookie sign-in: `POST {base}/login` with `{"username","password"}`; the browser keeps the cookies the answer sets. A non-success answer is mapped as above. |
| `sign_out(base)` | `POST {base}/logout` with body `{}`. Non-success mapped as above. |

Concurrent refresh: the refresh token rotates, so calls that meet an expiry at the same time share one refresh: the first sets `refreshing`, the others poll with `sleep(5)` (milliseconds) until the generation changes or the failure is recorded. A failure is remembered for the generation it happened in and returned to the waiting calls.
