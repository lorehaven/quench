---
title: Usage
parent: quench-http
grand_parent: Crates
nav_order: 1
---

Task-oriented examples, from the common to the advanced. Everything comes from `quench_http::prelude::*` unless an import is shown.

## 1. Routes and path parameters

Goal: answer `GET /users/42` with the id.

```rust
use quench_http::prelude::*;

#[get("/users/{id}")]
async fn get_user(Path(id): Path<u64>) -> String {
    format!("user #{id}")
}
```

Effect: `200 text/plain` with `user #42`. `GET /users/abc` answers `400` with body `invalid path parameter: ...` (the deserialiser's message). A path that matches but not for this method answers `405 method not allowed`; a path that matches nothing answers `404 not found`.

Pattern forms (see [internals](internals.md#route-patterns)): `{name}` one non-empty segment, `{name:.+}` one or more characters including `/`, `{name:.*}` zero or more. Docker-registry style:

```rust
#[derive(quench_serde::Deserialize)]
struct Blob { name: String, digest: String }

#[get("/{name:.+}/blobs/{digest}")]
async fn blob(Path(b): Path<Blob>) -> String {
    format!("{} {}", b.name, b.digest) // /library/ubuntu/blobs/sha256:ab -> "library/ubuntu sha256:ab"
}
```

Pitfalls: any other constraint (`{id:[0-9]+}`) panics when `discover_routes()` runs; trailing slashes are significant (`/ui` does not match `/ui/`); when two patterns can match the same path the one registered first wins.

## 2. Query string, JSON and form bodies

```rust
#[derive(quench_serde::Deserialize)]
struct Search { q: String, limit: Option<u32> }

#[get("/search")]
async fn search(Query(s): Query<Search>) -> String {
    format!("{} {}", s.q, s.limit.unwrap_or(10))
}

#[derive(quench_serde::Deserialize)]
struct NewUser { name: String }
#[derive(quench_serde::Serialize)]
struct User { id: u64, name: String }

#[post("/users")]
async fn create(Json(body): Json<NewUser>) -> Json<User> {
    Json(User { id: 1, name: body.name })
}

#[post("/login")]
async fn login(Form(f): Form<NewUser>) -> &'static str { "ok" }
```

Only one body-consuming extractor (`Json`, `Form`, `Bytes`, `Limited`, `Multipart`) per handler: the body is taken once; a second one sees an empty body. No `Content-Type` check is made. Malformed bodies answer `400` (also an over-size body: there is no `413`).

## 3. Dependency injection

```rust
use std::sync::Arc;

struct Config { label: &'static str }

#[injectable]
async fn config() -> Config { Config { label: "prod" } }

struct Repo { config: Arc<Config> }

#[injectable]
async fn repo(config: Arc<Config>) -> Repo { Repo { config } }

#[get("/label")]
async fn label(Inject(repo): Inject<Repo>) -> String { repo.config.label.to_string() }
```

Values built by hand are seeded before `build()`:

```rust
let container = ContainerBuilder::new()
    .provide(BodyLimit(64 * 1024 * 1024))
    .provide_arc(session_db)      // an Arc<SessionDb> you already hold
    .build()
    .await?;                      // DiError on a missing dependency or a cycle
```

`build()` fails before the server binds, with `DiError::MissingDependency` or `DiError::Cycle`. Pitfalls: `provide(arc)` keys the value as `Arc<T>`, not `T`: use `provide_arc`. Two `#[injectable]` functions returning the same type make `build()` fail with a misleading empty `Cycle` error (see the report).

## 4. Middleware

```rust
struct RequestLogger;

#[quench_http::async_trait]
impl Middleware for RequestLogger {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let line = format!("{} {}", req.method(), req.uri().path());
        let resp = next.call(req).await;
        println!("{line} -> {}", resp.status());
        resp
    }
}

let app = wrap(router, RequestLogger);
```

The last `wrap` call is the outermost layer: `wrap(wrap(inner, a), b)` runs `b`, `a`, `inner`. To limit a middleware to a path prefix: `wrap(app, OnPathPrefix::new("/api/", auth))`. Per-request data goes through `req.extensions_mut().insert(value)` and is read back with `Extension<T>` (`T: Clone`); a missing value answers `500`.

## 5. Mount under a runtime prefix

```rust
let base = std::env::var("BASE_PATH").unwrap_or_default(); // e.g. "/forge"
let app = mount(base, Arc::new(discover_routes()));
```

`""` and `"/"` mount at the root unchanged. Requests outside the prefix answer `404`. Do not nest `mount` inside `mount` and do not give a prefix a trailing slash (both 404 every request, see the report).

## 6. Body limits and uploads

```rust
// app-wide: every Json/Form/Bytes/Multipart
ContainerBuilder::new().provide(BodyLimit(1024 * 1024 * 1024));

// one route
#[post("/reports")]
async fn report(Limited(Json(r)): Limited<Json<Report>, 10_000_000>) -> &'static str { "ok" }

#[post("/upload")]
async fn upload(mut form: Multipart) -> Result<String, HttpError> {
    let mut size = 0;
    while let Some(mut field) = form.next_field().await? {
        if field.name() == Some("file") {
            while let Some(chunk) = field.chunk().await? { size += chunk.len(); }
        }
    }
    Ok(format!("{size} bytes"))
}
```

Default limit is `DEFAULT_BODY_LIMIT` = 2 MiB. Multipart's whole-stream cap is the same `BodyLimit`; unread fields are skipped.

## 7. Streaming responses, SSE, cookies

```rust
use quench_http::stream::receiver_stream;

#[get("/events")]
async fn events() -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(async move {
        let _ = tx.send(quench_http::http::sse::Event::new().event("tick").data("1")).await;
    });
    Response::sse(receiver_stream(rx))
}

#[get("/download")]
async fn download() -> Response {
    let file = tokio::fs::File::open("big.bin").await.unwrap();
    Response::streaming(quench_http::http::StatusCode::OK, quench_http::stream::reader_stream(file))
        .header("content-type", "application/octet-stream")
}
```

Cookies: `Response::ok().cookie(&cookie)` appends a `Set-Cookie` header (`quench_http::http::cookie::Cookie`).

## 8. WebSocket

```rust
use quench_http::ws::{Message, WebSocketUpgrade};

#[get("/socket")]
async fn socket(upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(|mut ws| async move {
        while let Ok(Some(m)) = ws.recv().await {
            if let Message::Text(t) = m { let _ = ws.send(Message::Text(t)).await; }
        }
    })
}
```

A request that is not a valid upgrade answers `400` before the handler runs. `WebSocketUpgrade` is not in the prelude.

## 9. Serving, TLS and shutdown

```rust
serve_http(app, container, addr).await?;

#[cfg(feature = "tls")]
if let Some(cfg) = load_tls("cert.pem", "key.pem") {
    serve_https(app, container, addr, cfg).await?;
}
```

`load_tls` returns `None` on missing or unreadable files so you can fall back to HTTP in development. For tests use `serve_http_on(app, container, listener, shutdown_future)` with a pre-bound `TcpListener` on port `0`.

## 10. Testing a handler without a socket

See [testing](testing.md).
