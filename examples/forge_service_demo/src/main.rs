//! A minimal forge-service-shaped demo: `quench-starter`'s bootstrap
//! (health/readiness, base-path scoping, graceful shutdown - see
//! `discover_and_mount`) plus `quench-auth`'s `Auth`/`RequireWrite`
//! middleware wrapped around it. This is the template for migrating a
//! real forge service, distilled to the parts that matter for that: no
//! database, no TLS, no gatehouse - see the comments below for what a
//! real service adds back in.
//!
//! Self-contained on purpose: `JwtConfig::for_tests_with_signing()`
//! mints its own in-process signing key instead of fetching gatehouse's
//! JWKS, and `POST /login` issues a token directly instead of redirecting
//! to gatehouse's login form - stand-ins so this runs with nothing else
//! started. A real service uses `JwtConfig::init()` (verifies against
//! `GATEHOUSE_URL`) and never issues its own tokens.
//!
//! Run with `cargo run -p quench-example-forge-service`, then:
//!
//! ```text
//! curl http://localhost:8080/health                          # no auth needed
//! curl http://localhost:8080/api/whoami                       # 401: no token
//! TOKEN=$(curl -s -X POST http://localhost:8080/login \
//!     -H 'content-type: application/json' \
//!     -d '{"username":"alice","roles":"user"}' | jq -r .access_token)
//! curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/whoami
//! curl -H "Authorization: Bearer $TOKEN" -X POST http://localhost:8080/api/notes \
//!     -H 'content-type: application/json' -d '{"text":"hi"}'   # 403: no write permission
//!
//! WRITE_TOKEN=$(curl -s -X POST http://localhost:8080/login \
//!     -H 'content-type: application/json' \
//!     -d '{"username":"bob","roles":"demo:write"}' | jq -r .access_token)
//! curl -H "Authorization: Bearer $WRITE_TOKEN" -X POST http://localhost:8080/api/notes \
//!     -H 'content-type: application/json' -d '{"text":"hi"}'   # 200
//! ```

use async_trait::async_trait;
use quench_auth::domain::jwt::{Claims, JwtConfig};
use quench_auth::domain::session::SessionDb;
use quench_auth::http::middleware::auth::Auth;
use quench_auth::http::middleware::require_write::RequireWrite;
use quench_cache::CacheStore;
use quench_http::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Applies `inner` only to requests whose path starts with `prefix`,
/// passing everything else straight through.
///
/// `Auth`/`RequireWrite` wrap the *whole* app they're given - correct when
/// a service really does gate everything behind auth, but this demo also
/// serves `/health` (a k8s probe has no token to send) and `/login` (its
/// whole job is handing out a token, so it can't require one). A real
/// service reaches for this same shape whenever only *some* scopes need a
/// particular middleware - see `warehouse-service`'s split between its
/// docker-registry scope (wrapped in auth) and its own root scope (isn't).
struct OnPathPrefix<M> {
    prefix: &'static str,
    inner: M,
}

#[async_trait]
impl<M: Middleware> Middleware for OnPathPrefix<M> {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if req.uri().path().starts_with(self.prefix) {
            self.inner.handle(req, next).await
        } else {
            next.call(req).await
        }
    }
}

/// Stands in for gatehouse's login form: issues a token for whatever
/// `username`/`roles` the caller asks for, no password check at all. A
/// real service never has an endpoint like this - it verifies tokens
/// gatehouse issued, it doesn't mint its own.
#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    /// Space/comma-separated roles, e.g. `"user"` or `"demo:write"` - the
    /// same shape a real token's `scope` claim carries (see
    /// `Claims::roles`/`Claims::can`).
    roles: String,
}

#[derive(Serialize)]
struct LoginResponse {
    access_token: String,
}

#[post("/login")]
async fn login(
    Json(req): Json<LoginRequest>,
    Inject(jwt_config): Inject<JwtConfig>,
) -> Result<Json<LoginResponse>, HttpError> {
    let token = jwt_config
        .issue_access_token(req.username, req.roles, None)
        .await
        .map_err(|e| HttpError::status(http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(LoginResponse {
        access_token: token,
    }))
}

#[derive(Serialize)]
struct WhoAmI {
    username: String,
    roles: Vec<String>,
}

/// Protected by `Auth` (wrapped around every `/api/*` path in `main`,
/// below): unreachable without a valid bearer token, and `Claims` -
/// `Auth`'s own output - is what makes the username available here via
/// `Extension`.
#[get("/api/whoami")]
async fn whoami(Extension(claims): Extension<Claims>) -> Json<WhoAmI> {
    Json(WhoAmI {
        username: claims.sub.clone(),
        roles: claims.roles(),
    })
}

#[derive(Deserialize)]
struct CreateNote {
    text: String,
}

/// Protected by `Auth` *and* `RequireWrite`: reachable with any valid
/// token, but `RequireWrite` 403s it unless the token carries `write` (or
/// a wildcard role) for this service - see the `roles` field in the
/// `/login` examples above.
#[post("/api/notes")]
async fn create_note(Extension(claims): Extension<Claims>, Json(note): Json<CreateNote>) -> String {
    format!("{} created a note: {}", claims.sub, note.text)
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt::init();

    // A real service reads this from `GATEHOUSE_URL`/`SERVICE_AUDIENCES`/...
    // via `JwtConfig::init()`. See this file's own doc comment for why the
    // demo mints its own key instead.
    envmnt::set("SERVICE_NAME", "demo");
    let mut jwt_config = JwtConfig::for_tests_with_signing();
    jwt_config.service_name = "demo".to_string();
    jwt_config.audiences = vec!["demo".to_string()];
    jwt_config.auth_enabled = true;

    let session_db: Arc<SessionDb> = SessionDb::init(CacheStore::in_memory());
    let health_state = quench_starter::common::health::HealthState::live();
    health_state.mark_ready(); // no async init step in this demo

    let container = ContainerBuilder::new()
        .provide(jwt_config.clone())
        .provide_arc(session_db)
        .provide(health_state)
        .build()
        .await
        .unwrap_or_else(|e| panic!("dependency graph failed to resolve: {e}"));
    let container = Arc::new(container);

    // `discover_and_mount` is the same piece `quench_starter::http::serve`
    // uses internally (health/readiness, base-path scoping, the
    // correlation-id + filtered-logger middleware) - exposed separately so
    // a service that needs its own middleware, like the `Auth`/`RequireWrite`
    // pair here, can wrap it before serving. A service with no such need
    // just calls `quench_starter::http::serve` directly instead of this.
    let app = quench_starter::http::discover_and_mount("/");
    let app = wrap(
        app,
        OnPathPrefix {
            prefix: "/api/",
            inner: RequireWrite::new(jwt_config.clone()),
        },
    );
    // Outermost: `Auth` has to run before `RequireWrite` so the `Claims`
    // it inserts exist by the time `RequireWrite` reads them - see
    // `quench_auth::http::middleware`'s doc comment. `/health` and
    // `/login` stay reachable with no token: neither path starts with
    // `/api/`.
    let app = wrap(
        app,
        OnPathPrefix {
            prefix: "/api/",
            inner: Auth::new(jwt_config),
        },
    );

    let addr = "127.0.0.1:8080".parse().unwrap();
    println!(
        "forge-service demo listening on http://{addr} - see src/main.rs for the curl walkthrough"
    );
    serve_http(app, container, addr).await
}
