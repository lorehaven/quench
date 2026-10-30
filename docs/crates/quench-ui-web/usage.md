---
title: Usage
parent: quench-ui-web
grand_parent: Crates
nav_order: 1
---

Task-oriented examples. All Rust runs inside the wasm module on one thread; futures run on the executor (`spawn`). Complete signatures are in the API pages.

## 1. Start an app

```rust
use quench_ui::prelude::*;

fn app() -> View {
    div().child("hello").into_view()
}
quench_ui_web::app!(app);
```

Serve `quench_ui_web::QUENCH_JS` as a JavaScript module and the built `.wasm`:

```html
<div id="app"></div>
<script type="module" src="/quench.js?wasm=/app.wasm"></script>
```

`app!` defines the `q_main` export; `run` installs the panic hook and the `WebPlatform`, then builds the view into `#app` (or `<body>`), or hydrates it when `#app` already has content. Pitfalls: the CSP must allow `'wasm-unsafe-eval'`; the server must send `.wasm` as `application/wasm` (otherwise `quench.js` falls back to a slower non-streaming load).

## 2. Read state the server embedded

```html
<script type="application/json" id="__q_state">{"user":"ann"}</script>
```

```rust
let state: Option<String> = quench_ui_web::initial_state();
// parse with the same types the server serialised, e.g. quench_serde::json::from_str
```

Available from `q_main` on. The text is exactly the element's `textContent`.

## 3. Mount and hydrate by hand

```rust
let mount = quench_ui_web::mount_root(app());        // client-only render
let (mount, mismatches) = quench_ui_web::hydrate_root(app()); // over server HTML
```

Call `hydrate_root` only when the server rendered the same view; `mismatches` is 0 when they agree. Keep the returned `Mount` alive, or call `.leak()` as `run` does.

## 4. Fetch JSON

```rust
use quench_ui_web::{Request, spawn};

spawn(async move {
    let response = Request::get("/api/notes").header("accept", "application/json").send().await;
    match response {
        Ok(r) if r.ok() => { let body = r.text(); /* parse */ }
        Ok(r) => { /* HTTP error: r.status */ }
        Err(e) => { /* network failure or abort: e */ }
    }
});
```

Abort from elsewhere: `let fetch = req.send(); let handle = fetch.abort_handle(); ... handle.abort();` then the future yields `Err(FetchError::Aborted)`. Dropping the future also aborts. Pitfall: `spawn` only queues; the task first runs at the next host tick.

## 5. Delay and timers

```rust
spawn(async {
    quench_ui_web::sleep(500).await;
    // after 500 ms
});
```

The timer starts at the first poll. `quench_ui::platform::after` (installed by `run`) uses the same machinery.

## 6. Live updates over SSE

```rust
use quench_ui_web::{EventSource, SseEvent, spawn};

let source = EventSource::open("/api/notes/stream", &["note"]);
spawn(async move {
    while let Some(event) = source.next().await {
        match event {
            SseEvent::Message { name, data } => { /* name == "note" or "message" */ }
            SseEvent::Reconnecting | SseEvent::Open => {}
            SseEvent::Closed => break,
        }
    }
});
```

List every named event you need in `names`; unnamed events (`message`) are always delivered. Moving the `EventSource` into the task ties its life to the task: when the task ends the source is dropped and closed.

## 7. Routing with the browser history

```rust
use quench_ui_web::BrowserHistory;
let history = BrowserHistory::new();   // pass it where quench_ui::router needs a History
```

`BrowserHistory` uses the URL the page was loaded at and follows back/forward. Which router entry point takes it is documented in [quench-ui](../quench-ui/api-router.md).

## 8. Remember a small value and the time zone

```rust
use quench_ui_web::browser;
browser::storage_set("theme", Some("dark"));
let theme = browser::storage_get("theme");   // also after a reload
let zone = browser::time_zone();             // "Europe/Warsaw" or "UTC"
let now = browser::wall_ms();
browser::storage_set("theme", None);         // forget
```

Keys are stored in `localStorage` under `q:`. An empty value forgets the key.

## 9. Call a host plugin

```rust
use quench_ui_web::plugins::{Device, Notifications};
use quench_ui_web::{BridgeError, bridge, spawn};

spawn(async {
    match Device::state().await {
        Ok(state) => { let _online = state.online; }
        Err(e) if e.code == "unavailable" => { /* plain browser: no host */ }
        Err(e) => { /* e.code, e.message */ }
    }
});
bridge::on_event(|plugin, event, _data| { if plugin == "device" && event == "state" { /* ... */ } });
```

Declare your own plugin with `#[quench_ui_web::plugin("echo")]` (see [quench-ui-web-macros](../quench-ui-web-macros/index.md)). Pitfalls: the first `on_event` call opens the event channel on the next tick, so events before then are lost; a call made before the host's port arrives waits up to 3 s.

## 10. Pointer and form specifics

A `submit` event never reloads the page. Other default actions are prevented only if the view asks (`Element::prevent_default`, router link interception). Touch arrives as pointer events; use CSS `touch-action` for gestures.
