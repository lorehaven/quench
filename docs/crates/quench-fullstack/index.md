---
title: quench-fullstack
parent: Crates
nav_order: 21
has_children: true
permalink: /crates/quench-fullstack/
---

`quench-fullstack` (library `quench_fullstack`) serves `quench-ui` apps from `quench-http` and lets them call server code. The page is rendered on the server from the same components the browser runs, the browser hydrates it (`quench-ui-web`), and everything goes out with content-hashed asset names and a strict Content-Security-Policy. A function marked `#[server]` (from `quench-fullstack-macros`, re-exported here) is an ordinary `async fn` to the code that calls it, in the browser or on the server. Sessions work by cookie in the browser and by bearer token in a native shell.

* Version: `0.1.7` (`quench-fullstack/Cargo.toml`). Library name `quench_fullstack`; `[lib] harness = false`.
* No `[features]` table. The split is by target: the crate builds for `wasm32` and for native targets, and the server half is `#[cfg(not(target_arch = "wasm32"))]`.

## Install

```toml
[dependencies]
quench-fullstack = "0.1"
quench-serde = "0.1"     # the #[server] expansion names ::quench_serde by path
```

The shared crate (built for the server and for the browser) needs both `quench-fullstack` and `quench-serde`, because the expansion refers to `::quench_serde::Serialize`, `::quench_serde::Deserialize` and `::quench_fullstack::...` by path.

## Targets and dependencies

| Target | Dependency | Why |
|---|---|---|
| all | `quench-core` | `async_trait`, `submit!`, `registry::Registry`, `hex`, `random`, `stream`. |
| all | `quench-fullstack-macros` | The `#[server]` attribute. |
| all | `quench-serde` | JSON, `Serialize`/`Deserialize`. |
| all | `quench-ui` | `View`, `render_to_string`, `form::Form`, `router`. |
| all | `quench-ui-web` | `fetch` host (`Request`, `Response`), `QUENCH_JS`, `sleep`. |
| not wasm32 | `quench-archive` | gzip of compressible assets. |
| not wasm32 | `quench-auth` (feature `http`) | `JwtConfig`, `Claims`, `SessionDb`, `realm`, session cookie lookup. |
| not wasm32 | `quench-crypto` | SHA-256 for `content_hash`. (`ring` does not build for wasm32.) |
| not wasm32 | `quench-http` | `Endpoint`, `Request`, `Response`, `FromRequest`, SSE. |
| not wasm32 | `quench-log` | Logs internal errors. |
| not wasm32 | `tokio` | `task_local!` for the current user, timers for the dev stream. |

Dev-dependencies: `quench-proto`, `quench-test`.

## Minimal example

Server:

```rust
use std::sync::Arc;
use quench_fullstack::{Page, Ui, UiApp};

let mut ui = Ui::new(std::fs::read("app.wasm")?);               // quench.js is added from quench-ui-web
let css = ui.add_asset("site.css", STYLE, "text/css");            // -> /_quench/site.<hash>.css
let app = UiApp::new(ui).page("/", move |_req| {
    Page::new("My app", my_app::view(None)).stylesheet(&css)      // rendered on the server
});
// serve `Arc::new(app)` with quench-http
```

Shared crate:

```rust
use quench_fullstack::{server, ServerError, user};

#[server(auth)]
pub async fn add_note(text: String) -> Result<Note, ServerError> {
    if text.trim().is_empty() {
        return Err(ServerError::invalid("text", "write something"));
    }
    Ok(store().add(&user()?.sub, text))
}
```

*Unverified:* the `quench_http` serving call is not shown; see `docs/crates/quench-http` for the host side.

## Where to go next

* [Usage](usage.md): pages, assets, server functions, errors, sessions, development reload.
* [API: root, errors, user, wire](api.md), [API: client](api-client.md), [API: server and testing](api-server.md), [API: pages, assets, `Ui`, `UiApp`](api-ui.md).
* [Configuration](configuration.md): environment variables, headers and constants.
* [Errors](errors.md): `ServerError`, its HTTP mapping and wire body.
* [Internals](internals.md): the call protocol, the security policy, the HTML layout, dev reload, favicon.
* [Testing](testing.md): the tests and `StubGatehouse`.
* The `#[server]` attribute itself: [quench-fullstack-macros](../quench-fullstack-macros/index.md).
