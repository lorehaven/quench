# quench-ui-web
Runs a quench-ui app in the browser from WebAssembly, with a small hand-written JS bridge.

## What it is
`quench-ui-web` runs a `quench-ui` app in the browser. The app compiles to WebAssembly (`wasm32-unknown-unknown`); one small hand-written JavaScript file, `quench.js` (embedded in the crate as `QUENCH_JS`), loads the module and gives it the DOM, timers, `fetch`, `EventSource`, history and storage. There is no `wasm-bindgen`, `web-sys` or `js-sys`.

The crate implements the `Renderer` trait of `quench-ui` for the DOM, an executor with `sleep`, `fetch` and `EventSource` futures, the router's `History`, the `Platform` trait, and typed clients of the Android and desktop host plugins (via `#[plugin]` from `quench-ui-web-macros`). On non-wasm targets an in-memory host lets most code run under `cargo test`.

## Use cases
- Start a `quench-ui` app in the browser, client-only or hydrating server-rendered HTML.
- Read state a server embedded in the page.
- Fetch JSON and stream server-sent events from async code.
- Call native Android or desktop host plugins.
- Test browser-facing code on the host with the mock.

## Example
```rust
use quench_ui::prelude::*;

fn app() -> View {
    div().child("hello").into_view()
}
quench_ui_web::app!(app);
```

```html
<div id="app"></div>
<script type="module" src="/quench.js?wasm=/app.wasm"></script>
```

Fetching from a spawned task:

```rust
use quench_ui_web::{Request, spawn};

spawn(async move {
    let response = Request::get("/api/notes").header("accept", "application/json").send().await;
    if let Ok(r) = response {
        if r.ok() { let body = r.text(); }
    }
});
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-ui-web/
