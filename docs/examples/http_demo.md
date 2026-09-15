# Example: Http Demo

`quench-example-http` (`examples/http_demo`) is a runnable tour of `quench-http` on its own — no `quench-starter` or `quench-auth` involved, just the framework. It's the companion to [`docs/quench-http.md`](../quench-http.md): every section of that guide has a working route or component here, and it's the fastest way to poke at the framework interactively rather than read about it.

## What it covers

- **Routing**: a static route (`/`), a `Path<T>` capture (`/users/{id}`), and a regex-constrained multi-segment wildcard (`/docker/{name:.+}/blobs/{digest}`) mirroring the docker registry API's shape - `name` swallows `library/ubuntu` (a `/`-containing segment) before `/blobs/{digest}` matches.
- **Extractors**: `Query<T>` (`/search`), `Json<T>` in and out (`/echo`), `Multipart` (`/upload`, a `note` text field plus a `file` field), and a `Json<T>` route deliberately capped by a tiny `BodyLimit` (`/big`) to make the 400 easy to trigger.
- **Dependency injection**: a `Counter` component registered with `#[injectable]` (no manual registration anywhere in `main`) and resolved per-request via `Inject<Counter>` at `/counter` - hit it twice to see the *same* instance's count climb.
- **Middleware**: a `RequestLogger` implementing `Middleware` as a plain `async fn`, wrapping the whole app via `wrap()`.
- **Graceful shutdown**: `Ctrl+C` drains in-flight connections instead of cutting them off - the log line for any request still in flight prints before the process exits.

## How to run it

```bash
cargo run -p quench-example-http
```

Then, with the server up on `http://127.0.0.1:8080`:

```bash
curl http://localhost:8080/
curl http://localhost:8080/users/42
curl "http://localhost:8080/search?q=hello&limit=5"
curl -X POST http://localhost:8080/echo -H 'content-type: application/json' -d '{"message":"hi"}'
curl http://localhost:8080/counter          # run twice, watch the count climb
curl http://localhost:8080/docker/library/ubuntu/blobs/sha256:deadbeef
curl -F "note=hello" -F "file=@Cargo.toml" http://localhost:8080/upload
curl -i -X POST http://localhost:8080/big -H 'content-type: application/json' \
    -d "{\"pad\": \"$(head -c 3000 </dev/zero | tr '\0' x)\"}"   # trips the 2KB demo limit
```

## Requirements

Just the Rust toolchain - no database, no external services. Dependencies are `quench-http`, `async-trait`, `serde`/`serde_json`, `tokio`, and `tracing`/`tracing-subscriber` (all workspace-managed).

[Home](../README.md)
