---
title: Mock HTTP server
parent: quench-test
grand_parent: Crates
nav_order: 4
---

Module `mock`: an HTTP/1.1 server on a free local port that answers from rules and records what it was asked (shaped like `wiremock`). The server runs on a thread and runtime of its own, so a test that blocks while waiting for a spawned process still gets answers. Rules are tried by priority (lower first, default 5) and, among equal priorities, in mounting order. A request no rule matches gets an empty `404 Not Found` (still recorded). Dropping the server checks the `expect`ed counts (not while panicking).

```rust,no_run
use quench_test::mock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};

# async fn demo() {
let server = MockServer::start().await;
Mock::given(method("GET"))
    .and(path("/health"))
    .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
    .expect(1)
    .mount(&server)
    .await;
let url = format!("{}/health", server.uri());
// ... call `url` with any client ...
server.verify().await;
# }
```

Verified behaviour (real run): a `POST /items` rule matching a header and JSON body answered `HTTP/1.1 201 Created` with `content-type: application/json`, `x-r: 1`, `content-length: 8` and `connection: close` when the request said `Connection: close`; an unmatched request got `404 Not Found` with `content-length: 0`.

## `struct MockServer`

`Debug` prints `MockServer(127.0.0.1:port)`. Not `Clone`.

```rust
impl MockServer {
    pub async fn start() -> Self
    #[must_use] pub fn uri(&self) -> String
    #[must_use] pub const fn address(&self) -> &SocketAddr
    pub async fn register(&self, mock: Mock)
    pub async fn received_requests(&self) -> Option<Vec<Request>>
    pub async fn reset(&self)
    pub async fn verify(&self)
}
impl Drop for MockServer
```

| Method | Behaviour |
|---|---|
| `start()` | binds `127.0.0.1:0` and starts serving. **Panics** if no port can be bound. |
| `uri()` | `http://127.0.0.1:<port>` |
| `address()` | the bound address |
| `register(mock)` | adds a rule (same as `Mock::mount`) |
| `received_requests()` | every request received, oldest first (`Some` always) |
| `reset()` | forgets all rules and recorded requests |
| `verify()` | checks every `expect`; **panics** with `mock server verification failed:` and one line per rule: ``mock `name` expected exactly 1 call(s) but got 0`` (`(unnamed)` when no `named`; the count text is `exactly N`, `between A and B` or `at least N`) |
| `Drop` | stops the serving thread, then (unless already panicking) panics with the same verification text |

All methods are `async` only to match the shape tests are written against; none awaits anything.

## `struct Request`

`#[derive(Debug, Clone)]`.

```rust
pub struct Request {
    pub method: String,
    pub path: String,                    // without the query
    pub query: String,                   // without the '?'
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
impl Request {
    #[must_use] pub fn header(&self, name: &str) -> Option<&str>          // first match, case-insensitive
    #[must_use] pub fn query_param(&self, name: &str) -> Option<String>    // percent-decoded, '+' is a space
    #[must_use] pub fn body_string(&self) -> String                        // lossy UTF-8
    #[must_use] pub fn body_json(&self) -> Option<Value>                   // None if not JSON
}
```

The server reads `Content-Length` and chunked bodies (up to 64 MiB, otherwise it closes the connection), answers `Expect: 100-continue` with `100 Continue`, and keeps connections alive for HTTP/1.1 unless `Connection: close`.

## `trait Match`

```rust
pub trait Match: Send + Sync + 'static { fn matches(&self, request: &Request) -> bool; }
impl<F: Fn(&Request) -> bool + Send + Sync + 'static> Match for F
```

A condition on a request. Closures work: `Mock::given(|r: &Request| r.path.starts_with("/v1/"))`.

## `struct ResponseTemplate`

`#[derive(Debug, Clone)]`.

```rust
impl ResponseTemplate {
    #[must_use] pub const fn new(status: u16) -> Self
    #[must_use] pub fn insert_header(self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self
    #[must_use] pub fn append_header(self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self
    #[must_use] pub fn set_body_string(self, body: impl Into<String>) -> Self
    #[must_use] pub fn set_body_json(self, body: impl quench_serde::Serialize) -> Self
    #[must_use] pub fn set_body_bytes(self, body: impl Into<Vec<u8>>) -> Self
    #[must_use] pub fn set_body_raw(self, body: impl Into<Vec<u8>>, mime: &str) -> Self
    #[must_use] pub const fn set_delay(self, delay: Duration) -> Self
}
```

| Method | Effect |
|---|---|
| `new(status)` | status line `HTTP/1.1 <status> <reason>` (reasons are known for 100, 200, 201, 202, 204, 206, 301, 302, 303, 304, 307, 308, 400, 401, 403, 404, 405, 409, 410, 413, 422, 429, 500, 501, 502, 503, 504; other codes get an empty reason) |
| `insert_header` | sets a header, replacing earlier ones of that name (case-insensitive) |
| `append_header` | adds a header, keeping earlier ones |
| `set_body_string` | body and `content-type: text/plain` |
| `set_body_json` | `quench_serde::json::to_string(&body)` and `application/json` (pass anything `Serialize`, including a `Value`) |
| `set_body_bytes` | `application/octet-stream` |
| `set_body_raw(body, mime)` | bytes and a given content type |
| `set_delay` | wait this long before writing the response |

A `content-length` header is added unless the template sets one; `connection: close` is added when the connection is not kept alive.

## `Times` and `Mock`

```rust
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Times { /* min, max */ }
impl From<u64> for Times          // exactly n
impl From<Range<u64>> for Times   // a..b means a to b-1
impl From<RangeInclusive<u64>> for Times
impl From<RangeFrom<u64>> for Times   // at least a
impl fmt::Display for Times       // "exactly N" | "between A and B" | "at least N"

pub struct MockBuilder  { .. }
impl MockBuilder {
    #[must_use] pub fn and(self, matcher: impl Match) -> Self
    #[must_use] pub fn respond_with(self, response: ResponseTemplate) -> Mock
}

pub struct Mock { .. }
impl Mock {
    #[must_use] pub fn given(matcher: impl Match) -> MockBuilder
    #[must_use] pub fn expect(self, times: impl Into<Times>) -> Self
    #[must_use] pub const fn up_to_n_times(self, n: u64) -> Self
    #[must_use] pub const fn with_priority(self, priority: u8) -> Self
    #[must_use] pub fn named(self, name: impl Into<String>) -> Self
    pub async fn mount(self, server: &MockServer)
}
```

`and` adds a condition (all must hold). `expect(times)` fixes the allowed call count, checked at `verify()` or drop. `up_to_n_times(n)` makes the rule stop matching after `n` calls so later rules take over. `with_priority(p)`: lower is tried first, default 5. `named` labels the rule in verification failures. `MockBuilder` and `Mock` implement `Debug`.

## `matchers`

All matchers are structs implementing `Match` with a function to build them (the struct names are public; the fields are not). Functions marked *panics* panic on an invalid pattern or JSON at construction.

| Function | Struct | Matches |
|---|---|---|
| `method(m)` | `MethodMatcher` | the HTTP method, case-insensitive |
| `path(p)` | `PathMatcher` | the exact path (query excluded) |
| `path_regex(pattern)` | `PathRegexMatcher` | path matching the regex anywhere (anchor with `^`/`$`); *panics* ``bad path pattern ..`` |
| `header(name, value)` | `HeaderMatcher` | header with exactly this value (name case-insensitive) |
| `header_regex(name, pattern)` | `HeaderRegexMatcher` | header value matching the regex; *panics* ``bad header pattern ..`` |
| `header_exists(name)` | `HeaderExistsMatcher` | header present |
| `query_param(name, value)` | `QueryParamMatcher` | decoded query parameter equal to the value |
| `body_string(text)` | `BodyStringMatcher` | body as text equal to `text` |
| `body_string_contains(text)` | `BodyContainsMatcher` | body as text contains `text` |
| `body_bytes(bytes)` | `BodyBytesMatcher` | body equal to the bytes |
| `body_json(value)` | `BodyJsonMatcher` | body parses as JSON equal to `quench_serde::to_value(&value)` (`value: Serialize`) |
| `body_json_string(json)` | `BodyJsonMatcher` | body parses as JSON equal to the JSON text; *panics* on bad JSON |
| `basic_auth(user, password)` | `BasicAuthMatcher` | `Authorization: Basic base64(user:password)` |
| `any()` | `AnyMatcher` | every request |
