# Quench UI Web

`quench-ui-web` (crate `quench_ui_web`) runs a [`quench-ui`](./quench-ui.md) app in a browser. The app compiles to a `.wasm` module; one small hand-written file, `quench.js` (under 300 lines, embedded in the crate as `quench_ui_web::QUENCH_JS`), loads it and gives it the DOM. There is no `wasm-bindgen`, `web-sys` or `js-sys`: the two sides talk through the plain functions below. The same bundle runs in a desktop browser, an Android `WebView` and a desktop `WebView`.

This page is the contract between the two halves. Changing either side means changing it here first.

## Boot

```html
<div id="app"></div>
<script type="module" src="/quench.js?wasm=/app.wasm"></script>
```

`quench.js` reads `?wasm=` from its own URL (so there is no inline script, which a strict Content-Security-Policy would need a nonce for), fetches and instantiates the module, then calls its `q_main` export. The policy must allow WebAssembly (`'wasm-unsafe-eval'` in `script-src`). Handle `1` is the root container: the element with id `app`, or `document.body` if there is none.

An app is a function returning a view:

```rust
use quench_ui::prelude::*;

fn app() -> View {
    let count = create_signal(0);
    // ...
    div().child("hello").into_view()
}
quench_ui_web::app!(app);
```

`app!` defines the `q_main` export. It installs the panic hook, builds the view into the root, and leaks the mount (the page owns it until it closes).

## Handles

A node is a `u32` handle into a table the JS side owns (a growable array with a free list). `0` means "none" and is never a valid handle; `1` is the root and is never released.

- Every handle returned by `q_create_*` belongs to Rust until Rust calls `q_release(h)`, exactly once. On the Rust side a handle is a reference-counted value whose last drop calls `q_release`, so a node is released when the view stops holding it (it left the DOM, or its effect was disposed).
- `q_release` frees the table slot and removes the listeners the JS side added for that handle. The slot is reused by a later `q_create_*`.
- A handle that is out of range, already released or of the wrong node type is ignored by every import: no exception, no effect. Likewise `q_dispatch_event` ignores a handle Rust does not know.

## Strings and buffers

All text is UTF-8 in linear memory.

- **Rust to JS (`ptr`, `len` arguments of an import):** borrowed, valid only until the import returns. JS must copy (decode) it before returning and must not keep the pointer. JS re-creates its `Uint8Array` view on `memory.buffer` for every call, because the memory may grow.
- **JS to Rust (`ptr`, `len` arguments of an export):** JS asks for room with `ptr = q_alloc(len)`, writes `len` bytes at `ptr`, and passes `(ptr, len)` to exactly one export. That export takes ownership and frees it; JS must not touch the buffer afterwards. If JS allocates and then decides not to call an export, it calls `q_dealloc(ptr, len)`. `q_alloc(0)` returns a non-null pointer that needs no write and is accepted by `q_dealloc`. A `len` of `0` in an export argument means "no data" and the pointer is ignored.
- Rust decodes with lossy UTF-8: invalid bytes become `U+FFFD` and never panic.

## Imports (Rust calls JS), module `"env"`

| Function | Meaning |
|---|---|
| `q_create_el(tag_ptr, tag_len) -> h` | `document.createElement` |
| `q_create_text(ptr, len) -> h` | `document.createTextNode` |
| `q_create_anchor() -> h` | an empty comment node, the marker of a dynamic region |
| `q_set_text(h, ptr, len)` | `node.data = text` |
| `q_set_attr(h, name_ptr, name_len, val_ptr, val_len)` | `setAttribute`; for `value`, `checked` and `selected` on form controls the property is set too, so a programmatic change shows after the user has typed |
| `q_remove_attr(h, name_ptr, name_len)` | `removeAttribute` (and the property reset for the three above) |
| `q_insert(parent, child, before)` | `parent.insertBefore(child, before)`; `before == 0` appends. An attached `child` moves |
| `q_remove(parent, child)` | `parent.removeChild(child)` if it is a child |
| `q_listen(h, kind)` | adds one listener of event `kind` (table below) on the node |
| `q_release(h)` | frees the handle and its listeners |
| `q_log(level, ptr, len)` | `0` debug, `1` info, `2` warn, `3` panic. Level 3 also shows a full-page overlay on `localhost`, `127.0.0.1`, `[::1]` or when `window.QUENCH_DEV` is true |
| `q_set_timeout(id, ms)` / `q_clear_timeout(id)` | `setTimeout`; when it fires JS calls `q_timer_fire(id)` |
| `q_fetch(id, method_ptr, method_len, url_ptr, url_len, headers_ptr, headers_len, body_ptr, body_len)` | `fetch`; headers are `Name: value` lines joined by `\n`; a `body_len` of `0` sends no body. Completion calls `q_fetch_done` |
| `q_fetch_abort(id)` | aborts that fetch (an `AbortController`); `q_fetch_done` is then called with status `0` |
| `q_schedule_tick()` | `queueMicrotask(q_tick)`; called when a task became ready to poll |

## Exports (JS calls Rust)

| Function | Meaning |
|---|---|
| `memory` | the instance's linear memory |
| `q_alloc(len) -> ptr`, `q_dealloc(ptr, len)` | buffers for text going in (see above) |
| `q_main()` | called once after instantiation |
| `q_dispatch_event(h, kind, ptr, len, a, b, c, flags)` | a listener fired on the node `h` |
| `q_tick()` | poll every task that is ready |
| `q_timer_fire(id)` | a timer fired |
| `q_fetch_done(id, status, body_ptr, body_len, headers_ptr, headers_len)` | a fetch finished. `status` is the HTTP status, or `0` for a network error or abort, in which case the body is a message. Body and headers are consumed buffers; headers are `name: value` lines |

### Events

| `kind` | Event | Payload |
|---|---|---|
| 0 | `click` | none |
| 1 | `dblclick` | none |
| 2 | `input` | `ptr/len`: the control's current value |
| 3 | `change` | `ptr/len`: the control's current value |
| 4 | `keydown` | `ptr/len`: `event.key`; `flags` |
| 5 | `keyup` | as `keydown` |
| 6 | `submit` | none |
| 7 | `focus` | none |
| 8 | `blur` | none |
| 9 | `pointerdown` | `a = clientX`, `b = clientY`, `c = pointerId` |
| 10 | `pointerup` | as `pointerdown` |
| 11 | `pointermove` | as `pointerdown` |

`flags` bits: `1` ctrl, `2` shift, `4` alt, `8` meta. Touch input arrives as pointer events; scrolling and zooming are controlled with CSS `touch-action`, not from Rust.

**Default actions.** JS calls `preventDefault` on `submit` events only, always, so a form never reloads the page. Nothing else is prevented: clicks on links navigate, keys type. A later stage (the router) adds an opt-in per listener.

**Delivery.** A DOM operation can raise events synchronously (removing the focused input fires `blur`). JS never delivers an event while one of its imports is running: it queues it and delivers it right after the import returns, so listeners cannot re-enter a half-finished update.

## Async

The wasm module has no threads and no runtime of its own. `quench_ui_web::spawn(future)` adds a task to a single-threaded executor. A task that is woken marks itself ready and, once per batch, calls `q_schedule_tick`; `q_tick` then polls every ready task. `sleep(ms)` and `fetch` are futures completed by `q_timer_fire` and `q_fetch_done`. Dropping a pending `sleep` clears its timer; dropping a pending fetch aborts it.

## Panics

`panic = "abort"` in the release profile. A panic first runs the hook, which sends the message to `q_log(3, ..)` (console, and the overlay in development); the instance then traps and every later export call throws, which `quench.js` catches and logs once.

## Build

```text
cargo build --target wasm32-unknown-unknown --profile wasm -p <app>
```

The workspace `wasm` profile is release with `opt-level = "z"`, `lto`, one codegen unit, `panic = "abort"` and `strip`. No `wasm-opt` or `wasm-bindgen`. Measured sizes are in the roadmap's Stage 4 findings.

## The Rust API

| | |
|---|---|
| `app!(my_app)` | defines the `q_main` export for a `fn() -> impl IntoView`; installs the panic hook, mounts into the root and leaks the mount |
| `DomRenderer`, `Node` | the `quench_ui::Renderer` for the DOM; `Node` is a reference-counted handle that is released when the last clone drops |
| `mount_root(view)`, `run(f)`, `install_panic_hook()` | the pieces `app!` is made of |
| `spawn(future)`, `sleep(ms)` | the single-threaded executor and its timer |
| `Request::get/post/new(..).header(..).body(..).send()` | starts a request now; the `Fetch` future yields `Response { status, headers, body }` (`ok()`, `text()`, `header(name)`) or a `FetchError` (`Network`, `Aborted`). `Fetch::abort_handle()` and dropping the future both abort |
| `QUENCH_JS` | the source of `quench.js`, for a server to serve |

A `value`, `checked` or `selected` attribute written by a view also sets the DOM property, so `attr_fn("value", ..)` keeps an input in step with a signal after the user has typed in it.

## Testing without a browser

On any target other than `wasm32` the crate swaps the JS side for an in-memory host (`quench_ui_web::mock`) that keeps the handle table, records every call and lets a test play the part of `quench.js`: fire an event, a timer or a fetch completion. The renderer, the event decoder, the executor, `sleep`, `fetch` and the buffer functions are the same code in both builds.

## Testing in a browser

`scripts/ui-web-test.fish` builds `examples/ui_web_app` for wasm, starts `examples/ui_web_server` (a `quench-http` server that sends a strict Content-Security-Policy), and runs `tests/browser.mjs`, which drives headless Chromium over the DevTools protocol (Node 22 and `chromium` needed). It checks that the app renders and fetches, that the counter patches the same text node, that submitting a form does not reload the page and clears the input, that keyed items keep their DOM nodes, that 100 items added and removed return the JS handle table to its starting size, that nothing was logged as an error, and that a panic shows the overlay and stops the app.

Unit tests (`tests/unit`) cover the renderer, handle release and reuse, 10,000 nodes coming and going, event decoding and flags, invalid UTF-8 and unknown handles or kinds, buffer allocation, the executor, timers and `fetch` including abort. Line coverage 96% on the host build; `host/wasm.rs` only compiles for `wasm32`.

Not covered: touch gestures and a real mobile `WebView` (no device was connected), and CSP rules for inline `style` attributes: `Element::style(..)` writes a `style` attribute, which a policy without `'unsafe-inline'` in `style-src-attr` blocks, so use classes or the stylesheet under a strict policy (Stage 5).

[Home](../README.md)
