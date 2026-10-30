---
title: Testing
parent: quench-ui-web
grand_parent: Crates
nav_order: 9
---

Two layers: unit tests that run under `cargo test` against the in-memory host ([API: mock host](api-mock.md)), and an end-to-end browser test.

## Unit tests with the mock host

On any target but `wasm32` the host calls (`create_element`, `fetch`, `set_timeout`, ...) are the mock. The renderer, the event decoder, the executor, `sleep`, `fetch`, `EventSource`, history, the bridge and the buffer functions are the same code in both builds. Pattern:

```rust
use quench_ui::prelude::*;
use quench_ui_web::mock;
use quench_ui_web::mount_root;

#[quench_test::test]
fn counter() {
    mock::reset();                                  // always first
    let count = create_signal(0);
    let (shown, up) = (count.clone(), count.clone());
    let view = div()
        .child(span().text_fn(move || format!("count: {}", shown.get())))
        .child(button().text("+").on_click(move || up.update(|c| *c += 1)))
        .into_view();
    let _mount = mount_root(view);
    assert_eq!(mock::html(), "<div><span>count: 0</span><button>+</button></div>");
    let button = mock::handles_by_tag("button")[0];
    assert!(mock::fire_click(button));
    assert_eq!(mock::text(), "count: 1+");
}
```

(From `tests/unit/dom_tests.rs`, where two clicks give `"count: 2+"`.)

Driving async code:

```rust
let slot = Rc::new(RefCell::new(None));
let s = slot.clone();
spawn(async move { *s.borrow_mut() = Some(Request::get("/x").send().await) });
mock::run_ticks();                                   // the task runs, the fetch is started
let id = mock::fetches()[0].id;
mock::complete_fetch(id, 200, b"hi", "content-type: text/plain");
```

Timers: `mock::timers()` then `mock::fire_timer(id)`. SSE: `mock::sse_sources()` then `mock::sse_emit(id, kind, name, data)`. Bridge: `mock::bridge_sent()` shows envelopes; `mock::bridge_deliver(r#"{"id":0,"ok":"hi"}"#)` answers; `mock::bridge_unavailable()` plays a page without a host. Plugin tests (`tests/unit/plugin_tests.rs`) use a helper `exchange` that checks the envelope (plugin, method, args), answers with a JSON `ok`, and returns the typed result.

Rules: call `mock::reset()` first in each test; tests use `quench_test::test` and the crate's custom harness (`harness = false`, `quench_test::main!()` in `tests/unit.rs`, which also has `quench_test::main!()` under `#[cfg(test)]` in `src/lib.rs`). The mock cannot simulate: hydration claiming (only the count you set with `set_server_content`), real focus, CSP, `localStorage`.

## Test files (`tests/unit/`)

`async_tests` (executor, `sleep`, `fetch`, abort), `bridge_tests` (envelopes, errors, events, `#[plugin]`), `browser_tests` (zone, storage), `dom_tests` (renderer, handles, events), `ffi_tests` (buffers, invalid UTF-8, unknown handles/kinds), `platform_tests`, `plugin_tests` (all 11 plugin clients), `router_tests` (`BrowserHistory`), `sse_tests`.

## Browser test

`tests/browser.mjs` (543 lines) drives headless Chromium over the DevTools protocol against `examples/ui_web_app` (built for wasm) and `examples/ui_web_server` (a `quench-http` server with a strict CSP). Run through `scripts/ui-web-test.fish`; it needs Node and `chromium` (override with `CHROMIUM`). Usage: `node quench-ui-web/tests/browser.mjs http://127.0.0.1:3101`. The old page lists what it checks (render and fetch, patching the same text node, form submit without reload, hydration with damaged server HTML). *Unverified:* the full check list and the script were not run for this page.

Not covered (from the old page): touch gestures and a real mobile WebView, and CSP rules for inline `style` attributes.
