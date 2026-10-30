---
title: quench-ui-web
parent: Crates
nav_order: 58
has_children: true
---

`quench-ui-web` (library `quench_ui_web`) runs a [`quench-ui`](../quench-ui/index.md) app in the browser. The app compiles to WebAssembly (`wasm32-unknown-unknown`); one small hand-written JavaScript file, `quench.js` (399 lines, embedded in the crate as `QUENCH_JS`), loads the module and gives it the DOM, timers, `fetch`, `EventSource`, history and storage. There is no `wasm-bindgen`, `web-sys` or `js-sys`: both sides talk through the plain functions documented in [Internals](internals.md). The crate implements the `Renderer` trait of `quench-ui` for the DOM, an executor with `sleep`/`fetch`/`EventSource` futures, the router's `History`, the `Platform` trait, and the typed clients of the Android/desktop host plugins.

Version: **0.1.7**. Edition: 2024 (workspace). No `rust-version` is set. License, authors and repository come from the workspace.

Everything runs on one thread. State is kept in `thread_local!` tables; views, mounts, nodes, futures and `EventSource` are `!Send`. On every target other than `wasm32` the JavaScript side is replaced by an in-memory host (`quench_ui_web::mock`), so nearly all code runs under `cargo test` (see [Testing](testing.md)).

## Install

```toml
[dependencies]
quench-ui = { version = "0.1.3", registry = "ennor" }
quench-ui-web = { version = "0.1.7", registry = "ennor" }
```

Inside the workspace use `quench-ui-web = { workspace = true }`. The workspace pins `version = "0.1.0"` with a `path`. Build the app as a `cdylib`:

```bash
cargo build --target wasm32-unknown-unknown --profile wasm -p <app>
```

The `wasm` profile is defined in the workspace manifest, not in this crate. *Unverified:* its exact settings (the old page said release with `opt-level = "z"`, `lto`, one codegen unit, `panic = "abort"`, `strip`) were not re-checked here.

## Features

None. The crate has no `[features]` table. The `wasm32` / non-`wasm32` split is by `cfg(target_arch = "wasm32")`.

## Dependencies

| Crate | Why |
|---|---|
| `quench-ui` | `Renderer`, `View`, `Event`, `Mount`, `router::History`, `platform::Platform`: what this crate implements. |
| `quench-serde` | JSON for the plugin bridge (`json`, `Value`, `Serialize`, `Deserialize`) and for `q_set_env`; the plugin macro expands to `quench_ui_web::__private::json`. |
| `quench-ui-web-macros` | The `#[plugin]` attribute, re-exported as `quench_ui_web::plugin`. See [quench-ui-web-macros](../quench-ui-web-macros/index.md). |
| `quench-test` (dev) | The test harness (`harness = false`, `quench_test::main!()`). |

No `unsafe` outside two narrow spots: `#[unsafe(no_mangle)]` on the exports (`src/ffi.rs`) and the `unsafe extern "C"` import block (`src/host/wasm.rs`); both files carry `#![allow(unsafe_code)]`. Addresses cross the boundary as `usize` numbers and are never dereferenced on the Rust side.

The package `include`s `src/**`, `js/**` and `Cargo.toml`. The crate has `autotests = false`; the library and the `unit` test both use `harness = false`.

## Where to go next

* [Usage](usage.md): an app, server page, state, fetch, SSE, router, plugins.
* [API: core](api-core.md): the root items, `app!`, DOM renderer, events, executor, history, platform, `browser`, the `q_*` exports.
* [API: net](api-net.md): `Request`, `Response`, `Fetch`, `EventSource`.
* [API: bridge](api-bridge.md): `bridge`, `BridgeError`.
* [API: plugins](api-plugins.md): the 11 typed plugin clients and their data types.
* [API: mock host](api-mock.md): `quench_ui_web::mock` for tests.
* [Configuration](configuration.md): how `quench.js` is loaded and configured.
* [Internals](internals.md): the exact JS to wasm contract, hydration, bridge protocol.
* [Testing](testing.md): unit tests with the mock host, the browser test.
* [`#[plugin]`](../quench-ui-web-macros/index.md).

## Minimal example

`src/lib.rs` of a `cdylib` crate (the code of the crate-level doc example and `tests/unit/dom_tests.rs::counter`):

```rust
use quench_ui::prelude::*;

fn app() -> View {
    let count = create_signal(0);
    let (shown, bump) = (count.clone(), count.clone());
    div()
        .child(span().text_fn(move || format!("count: {}", shown.get())))
        .child(button().text("+").on_click(move || bump.update(|c| *c += 1)))
        .into_view()
}

quench_ui_web::app!(app);
```

The page that loads it:

```html
<div id="app"></div>
<script type="module" src="/quench.js?wasm=/app.wasm"></script>
```

The server answers `/quench.js` with `quench_ui_web::QUENCH_JS` (content type `text/javascript`) and `/app.wasm` with the module (`application/wasm`).
