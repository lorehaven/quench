//! quench-http feature tour: routing (including the regex-constrained
//! wildcard segments actix-router keeps working), annotation-discovered
//! components and routes, a real dependency-injection graph, extractors
//! (`Path`/`Query`/`Json`/`Inject`/`Multipart`), a configurable body-size
//! limit, plain-async middleware, and graceful shutdown - no quench-auth
//! or quench-starter here, just the framework itself. See
//! `examples/forge_service_demo` for the fuller production-bootstrap
//! shape (health checks, base-path scoping, auth middleware) this tour
//! deliberately leaves out.
//!
//! Run with `cargo run -p quench-example-http`, then try:
//!
//! ```text
//! curl http://localhost:8080/
//! curl http://localhost:8080/users/42
//! curl "http://localhost:8080/search?q=hello&limit=5"
//! curl -X POST http://localhost:8080/echo -H 'content-type: application/json' -d '{"message":"hi"}'
//! curl http://localhost:8080/counter          # run twice, watch the DI-shared counter climb
//! curl http://localhost:8080/docker/library/ubuntu/blobs/sha256:deadbeef
//! curl -F "note=hello" -F "file=@Cargo.toml" http://localhost:8080/upload
//! curl -i -X POST http://localhost:8080/big -H 'content-type: application/json' \
//!     -d "{\"pad\": \"$(head -c 3000 </dev/zero | tr '\0' x)\"}"   # trips the 2KB demo limit
//! ```
//!
//! Then Ctrl+C: the log line for any request still in flight prints
//! *before* the process exits, proving the shutdown drains it instead of
//! cutting it off.

use async_trait::async_trait;
use quench_http::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// ---------------------------------------------------------------------
// DI: a shared, in-memory counter every request can see and increment.
// `#[injectable]` discovers this automatically - `main()` never registers
// it by hand, the same way a real service's `UserDb`/`SessionDb` show up
// in the container without an explicit `.provide()` call for each one.
// ---------------------------------------------------------------------

struct Counter(AtomicU64);

#[injectable]
async fn provide_counter() -> Counter {
    Counter(AtomicU64::new(0))
}

// ---------------------------------------------------------------------
// Routes - each just an annotated async fn; nothing calls these by name.
// ---------------------------------------------------------------------

#[get("/")]
async fn hello() -> &'static str {
    "quench-http feature tour - see src/main.rs for the curl commands"
}

/// `Path<T>` - captures `{id}` and deserializes it into `T`.
#[get("/users/{id}")]
async fn get_user(Path(id): Path<u64>) -> String {
    format!("user #{id}")
}

/// `Query<T>` - deserializes the whole query string into a struct.
#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<u32>,
}

#[get("/search")]
async fn search(Query(query): Query<SearchQuery>) -> String {
    format!(
        "searching for {:?}, limit {}",
        query.q,
        query.limit.unwrap_or(10)
    )
}

/// `Json<T>` in, `Json<T>` out.
#[derive(Deserialize)]
struct EchoRequest {
    message: String,
}

#[derive(Serialize)]
struct EchoResponse {
    you_said: String,
}

#[post("/echo")]
async fn echo(Json(body): Json<EchoRequest>) -> Json<EchoResponse> {
    Json(EchoResponse {
        you_said: body.message,
    })
}

/// `Inject<T>` - resolves the DI-managed `Counter`. Every request sees the
/// *same* instance (built once at startup, not per-request).
#[get("/counter")]
async fn counter(Inject(counter): Inject<Counter>) -> String {
    let n = counter.0.fetch_add(1, Ordering::Relaxed) + 1;
    format!("this counter has been hit {n} time(s) since startup")
}

/// A regex-constrained, multi-segment wildcard path - the reason
/// quench-http keeps actix-router instead of a plain radix-tree matcher.
/// `name` swallows however many `/`-separated segments come before
/// `/blobs/...`, mirroring the docker registry API's
/// `/{name:.+}/blobs/{digest}` (see `warehouse-service`).
#[derive(Deserialize)]
struct DockerBlobParams {
    name: String,
    digest: String,
}

#[get("/docker/{name:.+}/blobs/{digest}")]
async fn docker_blob(Path(params): Path<DockerBlobParams>) -> String {
    format!(
        "would serve blob {} for image {}",
        params.digest, params.name
    )
}

/// `Multipart` - field-by-field, the same shape `sage-service`'s file
/// upload needs (a `file` field plus an optional text field).
#[post("/upload")]
async fn upload(mut form: Multipart) -> Result<String, HttpError> {
    let mut note = None;
    let mut file_name = None;
    let mut file_size = 0usize;

    while let Some(field) = form.next_field().await? {
        match field.name() {
            Some("note") => note = Some(field.text().await?),
            Some("file") => {
                file_name = field.file_name().map(str::to_string);
                file_size = field.bytes().await?.len();
            }
            _ => {}
        }
    }

    Ok(format!(
        "note={:?}, file={:?} ({file_size} bytes)",
        note.unwrap_or_default(),
        file_name.unwrap_or_default()
    ))
}

/// `Json<T>` respects the app-wide [`BodyLimit`] `main()` provides below -
/// set deliberately tiny here to make the 400 easy to trigger. A real
/// service raises this instead (see `BodyLimit`'s own doc comment for the
/// warehouse-service upload case that motivated it).
#[post("/big")]
async fn big(Json(_body): Json<serde_json::Value>) -> &'static str {
    "somehow made it under the 2KB demo limit"
}

// ---------------------------------------------------------------------
// Middleware - a plain async fn wrapping the rest of the app, unlike
// actix-web's Service/Transform pair.
// ---------------------------------------------------------------------

struct RequestLogger;

#[async_trait]
impl Middleware for RequestLogger {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let method = req.method().clone();
        let path = req.uri().path().to_string();
        let response = next.call(req).await;
        tracing::info!("{method} {path} -> {}", response.status());
        response
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    // Every #[injectable]/#[get]/#[post] linked into this binary is
    // discovered automatically - nothing above this line is registered by
    // hand.
    let container = ContainerBuilder::new()
        .provide(BodyLimit(2 * 1024))
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let app = wrap(router, RequestLogger);

    let addr = "127.0.0.1:8080".parse().unwrap();
    println!(
        "quench-http demo listening on http://{addr} - Ctrl+C to see graceful shutdown drain an in-flight request"
    );
    serve_http(app, container, addr).await
}
