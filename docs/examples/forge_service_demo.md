# Example: Forge Service Demo

`quench-example-forge-service` (`examples/forge_service_demo`) is a minimal forge-service-shaped app: `quench-starter`'s bootstrap (health/readiness, base-path scoping, graceful shutdown) plus `quench-auth`'s `Auth`/`RequireWrite` middleware wrapped around it. Where `http_demo` is a tour of `quench-http` alone, this is the template for what a real service's `main.rs` looks like once it's migrated onto the quench-http stack - distilled to the parts that matter for that (no database, no TLS, no real gatehouse).

## What it covers

- `quench_starter::http::discover_and_mount(base_path)` - the composable half of `quench_starter::http::serve()` (health/readiness routes, base-path scoping, the correlation-id/logger middleware), exposed separately so a service that needs its own middleware around all of that - auth, here - can add it before serving. A service with no such need calls `serve()` directly instead.
- `quench_auth::http::middleware::{Auth, RequireWrite}`, scoped to `/api/*` only via a small `OnPathPrefix` combinator - `/health` and `/login` stay reachable with no token, the same split a real service draws between its public and authenticated scopes (see `warehouse-service`'s root scope vs. its docker-registry scope for the actix-era version of the same idea).
- `Extension<Claims>` reading the identity `Auth` decoded, in both a read-only route (`/api/whoami`) and a write-gated one (`/api/notes`, which `RequireWrite` 403s without a `write`-scoped token).
- The DI container seeded with `provide`/`provide_arc`: a `JwtConfig` (bare value), a `SessionDb` (already behind an `Arc` from `SessionDb::init`, hence `provide_arc`), and a `HealthState`.

**Self-contained by design**: `JwtConfig::for_tests_with_signing()` mints its own in-process signing key instead of fetching gatehouse's JWKS, and `POST /login` issues a token directly instead of redirecting to gatehouse's login form - stand-ins so this runs with nothing else started. A real service uses `JwtConfig::init()` (verifies against `GATEHOUSE_URL`) and never issues its own tokens; see [Quench Auth](../quench-auth.md) for what it does instead.

## How to run it

```bash
cargo run -p quench-example-forge-service
```

Then, with the server up on `http://127.0.0.1:8080`:

```bash
curl http://localhost:8080/health                          # no auth needed

curl http://localhost:8080/api/whoami                       # 401: no token

TOKEN=$(curl -s -X POST http://localhost:8080/login \
    -H 'content-type: application/json' \
    -d '{"username":"alice","roles":"user"}' | jq -r .access_token)
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/whoami

curl -H "Authorization: Bearer $TOKEN" -X POST http://localhost:8080/api/notes \
    -H 'content-type: application/json' -d '{"text":"hi"}'   # 403: no write permission

WRITE_TOKEN=$(curl -s -X POST http://localhost:8080/login \
    -H 'content-type: application/json' \
    -d '{"username":"bob","roles":"demo:write"}' | jq -r .access_token)
curl -H "Authorization: Bearer $WRITE_TOKEN" -X POST http://localhost:8080/api/notes \
    -H 'content-type: application/json' -d '{"text":"hi"}'   # 200
```

## Requirements

Just the Rust toolchain - no database, no gatehouse, no TLS certs. Dependencies are `quench-http`, `quench-starter`, `quench-auth`, `quench-cache` (for an in-memory session store), and the usual `tokio`/`serde`/`tracing` set (all workspace-managed).

[Home](../README.md)
