---
title: Internals
parent: quench-starter
grand_parent: Crates
nav_order: 7
---

## Composition of `serve`

```text
FilteredLogger -> CorrelationId -> SecurityHeaders -> RootOrMounted -> mount(base_path, discover_routes())
```

`serve_app` then runs HTTPS (+ redirect server) or plain HTTP.

## Why routes are referenced by name

`health`, `swagger` and `ui` register their routes with `#[get]` (a before-`main` registry). `quench-starter` is a dependency, so its object code is linked lazily; without a real reference a binary could lose these routes. `discover_and_mount` calls the private `register_routes()` of each module, which names the handlers, to keep them linked.

## Registration-order caveat

`base_path_redirect` (`""`) and `base_path_slash_redirect` (`"/"`) go through the same `discover_routes()` as the service's own routes. There is no guaranteed precedence between them and a route a service registers at the same pattern (a root-mounted `{path:.*}` catch-all can match the bare patterns). In practice this crate's registrations link before a dependent service's, so they win, but that rests on linker behaviour (source: `src/http/routers/ui.rs` module comment). A service that needs its own routing at the literal base path should register more specifically than a bare root wildcard.

## Why `server_root_redirect` is separate

When `BASE_PATH` is not `/`, a request for `/` does not start with the base path and never reaches the mounted router. Registering it as a discovered route would collide with the mounted router's own `"/"` route, so `RootOrMounted` answers it directly. When `BASE_PATH` is `/`, `mount` delegates unchanged and `base_path_slash_redirect` answers `GET /`.

## Process-wide base path

`common::routes` stores the normalised `BASE_PATH` in a `LazyLock` on first use. `ui_path`, `with_base_path` and the swagger redirects use it; `serve` reads `BASE_PATH` again for the mount. Changing the variable after the first call has no effect on `with_base_path`.

## Redirect server

Answers all requests with `308` and `Location: https://{host}{:port}{uri}`; the `Host` header value is parsed as a URI authority to render the host.

## Health state

Two atomics (`Acquire`/`Release` ordering). Liveness is always true for a `live()` state. Readiness is set once by `mark_ready`.

## Environment

`logging::init` writes variables from `.env` files into the process environment (`quench_core::env::set`) so that libraries reading `std::env` see them.
