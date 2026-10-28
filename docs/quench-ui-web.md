# Quench UI Web

`quench-ui-web` (crate `quench_ui_web`) runs a [`quench-ui`](./quench-ui.md) app in a browser. The app compiles to a `.wasm` module; one small hand-written file, `quench.js` (under 400 lines, embedded in the crate as `quench_ui_web::QUENCH_JS`), loads it and gives it the DOM. There is no `wasm-bindgen`, `web-sys` or `js-sys`: the two sides talk through the plain functions below. The same bundle runs in a desktop browser, an Android `WebView` and a desktop `WebView`.

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

`app!` defines the `q_main` export. It installs the panic hook, then builds the view into the root and leaks the mount (the page owns it until it closes). If the root already holds content, the page was rendered on the server and the view is *hydrated* instead of built (see below).

Before `q_main`, `quench.js` reads the text of `<script type="application/json" id="__q_state">`, if the page has one, and hands it to the module with `q_set_state`; the app reads it with `quench_ui_web::initial_state()`.

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
| `q_sse_open(id, url_ptr, url_len, names_ptr, names_len)` | `new EventSource(url)`, listening for the unnamed event (`message`) and for each event name in `names` (separated by `\n`); reports through `q_sse_event` |
| `q_sse_close(id)` | closes that `EventSource`; nothing is reported afterwards |
| `q_root_has_content() -> u32` | `1` if the root has any child node, i.e. the server rendered into it |
| `q_hydrate_begin()` | from now on, creating a node claims the next server-rendered node (see Hydration) |
| `q_prevent(h, kind, mode)` | for the listeners of `kind` on the node: `mode` `1` always calls `preventDefault`; `2` is for links: a plain left click is handled and prevented, while a ctrl-, shift-, alt-, meta- or non-primary click is neither handled nor prevented, so the browser opens the link as it normally would. Called after `q_listen` |
| `q_set_style(h, name_ptr, name_len, val_ptr, val_len)` | `node.style.setProperty` (an empty value removes the property). Through the style object, not the `style` attribute, so it works under `style-src 'self'` |
| `q_now() -> f64` | `performance.now()`, milliseconds |
| `q_reduced_motion() -> u32` | `1` if `prefers-reduced-motion: reduce` matches |
| `q_focus(h)` | focuses the node once the current call ends (it is attached by then) |
| `q_trap_focus(h)` | Tab and Shift+Tab move between the focusable things inside the node and wrap around, until the handle is released |
| `q_history(op, ptr, len)` | `0` `pushState` of the URL (the scroll position of the entry being left is saved first, then the page scrolls to the top), `1` `replaceState`, `2` `history.back()`, `3` `history.forward()` |
| `q_hydrate_end() -> u32` | stops claiming; removes server nodes nothing claimed; returns how many mismatches there were |

## Exports (JS calls Rust)

| Function | Meaning |
|---|---|
| `memory` | the instance's linear memory |
| `q_alloc(len) -> ptr`, `q_dealloc(ptr, len)` | buffers for text going in (see above) |
| `q_set_state(ptr, len)` | the page's embedded JSON, called before `q_main` when there is any |
| `q_set_location(ptr, len)` | the page's URL (path, query, hash), called before `q_main` |
| `q_popstate(ptr, len)` | the browser moved through its history; the URL it is at now. JS then restores the scroll position saved in that entry (`history.scrollRestoration` is `"manual"`) |
| `q_main()` | called once after instantiation |
| `q_dispatch_event(h, kind, ptr, len, a, b, c, flags) -> u32` | a listener fired on the node `h`. Returns `1` if a listener called `quench_ui::prevent_default()`; JS then calls `preventDefault` on the event (when delivery was not deferred) |
| `q_tick()` | poll every task that is ready |
| `q_timer_fire(id)` | a timer fired |
| `q_sse_event(id, kind, name_ptr, name_len, data_ptr, data_len)` | something happened to an `EventSource`: `kind` `0` it opened, `1` an event arrived (`name`: its event name, `message` if unnamed; `data`), `2` the connection was lost and the browser is reconnecting, `3` it failed for good (`readyState` closed). Both buffers are consumed; they are empty except for `1` |
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
| 12 | `compositionstart` | `ptr/len`: the control's value |
| 13 | `compositionend` | `ptr/len`: the control's value, which now has the committed text |
| 14 | `scroll` | `a = scrollTop`, `b = scrollLeft` of the element |
| 15 | `transitionend` | none |
| 16 | `resize` | `a`, `b`: the element's content width and height, reported when it is first laid out and on every change (a `ResizeObserver`) |
| 17 | `pointercancel` | as `pointerdown` |

`flags` bits: `1` ctrl, `2` shift, `4` alt, `8` meta. Touch input arrives as pointer events; scrolling and zooming are controlled with CSS `touch-action`, not from Rust.

**Default actions.** JS calls `preventDefault` on `submit` events, always, so a form never reloads the page. Nothing else is prevented unless the app asks with `q_prevent`: clicks on links navigate, keys type. The router asks for links (`Element::intercept_click`), and `Element::prevent_default(kind)` asks for the rest.

**Delivery.** A DOM operation can raise events synchronously (removing the focused input fires `blur`). JS never delivers an event while one of its imports is running: it queues it and delivers it right after the import returns, so listeners cannot re-enter a half-finished update.

## Hydration

A page rendered on the server already has the DOM. Hydration builds the same view over it and keeps its nodes, so nothing flashes, no node is duplicated, and a node the browser has already painted keeps its identity. The mount engine creates nodes **in document order** (an element before its children, a dynamic or keyed region's content before the anchor that ends it), so `quench.js` only needs a cursor.

Between `q_hydrate_begin` and `q_hydrate_end`, `quench.js` keeps a stack of frames, one per element being built, each with the parent and the next unclaimed child (the cursor):

- `q_create_el(tag)`: if the cursor is an element with that tag, claim it and push a frame for its children. Otherwise create a fresh element (a mismatch).
- `q_create_text(s)`: claim a text node whose data is `s`. If the node holds `s` followed by more, split it: the HTML parser merges adjacent text, so no separator is needed in the markup. Empty text has no markup, so an empty text node is created and placed at the cursor.
- `q_create_anchor()`: claim an empty comment (`<!---->`).
- `q_insert(parent, child, before)`: a claimed node is already in place (an element's frame is popped, and server nodes left unclaimed inside it are removed). A fresh node goes at the cursor and **replaces** the node that did not match.
- `q_set_attr`: an attribute that already has the value is left alone; one that differs on a claimed node is a mismatch and is set.

The result is always what a client-only render would have produced, because every mismatch is repaired by building the client's node. In development (`dev`) each mismatch logs `quench: hydration mismatch in #app > div:nth-child(2): expected <h2>, found <h3>; rebuilt it` with the path of the parent; in production nothing is logged. The Rust side learns the count from `q_hydrate_end` (`hydrate_root` returns it).

What the server must write so this works (`quench-ui`'s serialiser does all of it): elements with their attributes in the order the view sets them, text exactly as the view has it, an empty comment for each dynamic or keyed region, a doubled newline after `<pre>` and `<textarea>` when the text begins with one (the parser drops the first), and no content inside void elements. Markup the browser re-parents (a `<div>` inside a `<p>`, a `<tr>` without a `<tbody>`) will mismatch and be repaired.

## Async

The wasm module has no threads and no runtime of its own. `quench_ui_web::spawn(future)` adds a task to a single-threaded executor. A task that is woken marks itself ready and, once per batch, calls `q_schedule_tick`; `q_tick` then polls every ready task. `sleep(ms)` and `fetch` are futures completed by `q_timer_fire` and `q_fetch_done`. Dropping a pending `sleep` clears its timer; dropping a pending fetch aborts it.

## Server-sent events

`EventSource` is the browser's own client for `text/event-stream`: it reconnects by itself and sends `Last-Event-ID`. Its events are named, and a browser can only listen for names it is told, so `q_sse_open` takes the names the app wants. Events reach Rust through `q_sse_event`; the Rust side keeps them in a queue per source, so none is lost while the app is busy. Closing is idempotent, and dropping a Rust `EventSource` closes it.

## The host bridge

A host app that shows the bundle in a `WebView` (the [Android host](./quench-android.md)) can offer native plugins to the app. The page and the host talk over one `MessagePort`, and the module reaches it **through the imports it already has**: there is no bridge-specific function in the contract.

- **Calls are fetches.** `q_fetch` with the address `quench-bridge:call` and the JSON envelope as the body is a call. `quench.js` recognises the address and sends the body through the port instead of the network; the fetch completes (status `200`, the answer as the body) when the answer with the same `id` arrives, and with status `0` and a message when there is no host (`this page is not shown by a host app`) or it did not connect within 3 s. Aborting the fetch forgets the call.
- **Events are an event source.** `q_sse_open` with the address `quench-bridge:events` registers the module as the receiver of the host's pushed events: each arrives as a `q_sse_event` named `event` whose data is the JSON message. A source that is closed stops receiving them.
- **Detection.** The host appends `QuenchHost/1` to the user agent, so a page knows at once whether a host exists. In a plain browser every call fails immediately (`unavailable`).
- **Connecting.** When a page has loaded, the host posts the message `"quench-bridge"` with one port to the window, addressed to the page's origin (`WebView.postWebMessage` delivers it to that origin only). `quench.js` listens for it from the start and takes the port **only** from an event that is trusted, has no source window and an empty origin: that is how a native post looks, and a script's or another window's `postMessage`, or a synthetic `MessageEvent`, looks different. A later port replaces the earlier one (a reload).
- **Envelope.** A call is `{"id":7,"plugin":"echo","method":"say","args":{"text":"hi"}}`, the answer `{"id":7,"ok":<value>}` or `{"id":7,"err":{"code":"permission_denied","message":"..."}}`; an event is `{"event":"progress","plugin":"files","data":{..}}`. `id` matches them; answers may come in any order and from any thread. The host answers a call to a plugin it does not have with `unknown_plugin`, and arguments a method cannot read with `invalid_args`; error codes are otherwise the plugin's own. Messages over 1 MiB are dropped.
- **Rust.** `quench_ui_web::bridge::call(plugin, method, &args)` makes a call; `#[quench_ui_web::plugin("echo")] trait Echo { async fn say(text: String) -> Result<String, BridgeError>; }` generates the typed client (`Echo::say("hi".into()).await`); `bridge::on_event(|plugin, event, data| ..)` listens for pushed events. Dropping the future forgets the call, and a late answer is ignored. See [`quench-ui-web-macros`](./quench-ui-web-macros.md).

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
| `mount_root(view)`, `hydrate_root(view)`, `run(f)`, `install_panic_hook()` | the pieces `app!` is made of; `hydrate_root` returns the mount and the number of mismatches |
| `initial_state()` | the JSON the server embedded in the page, if any |
| `spawn(future)`, `sleep(ms)` | the single-threaded executor and its timer |
| `EventSource::open(url, &["event-name"])` | a server-sent events stream: `while let Some(event) = source.next().await` yields `SseEvent::{Open, Message { name, data }, Reconnecting, Closed}`; `close()` or dropping it ends it |
| `Request::get/post/new(..).header(..).body(..).send()` | starts a request now; the `Fetch` future yields `Response { status, headers, body }` (`ok()`, `text()`, `header(name)`) or a `FetchError` (`Network`, `Aborted`). `Fetch::abort_handle()` and dropping the future both abort |
| `bridge::call(..)`, `bridge::on_event(..)`, `BridgeError`, `#[plugin("name")]` | calls to the host app's native plugins, and the events they push |
| `QUENCH_JS` | the source of `quench.js`, for a server to serve |

A `value`, `checked` or `selected` attribute written by a view also sets the DOM property, so `attr_fn("value", ..)` keeps an input in step with a signal after the user has typed in it.

## Testing without a browser

On any target other than `wasm32` the crate swaps the JS side for an in-memory host (`quench_ui_web::mock`) that keeps the handle table, records every call and lets a test play the part of `quench.js`: fire an event, a timer or a fetch completion. The renderer, the event decoder, the executor, `sleep`, `fetch`, the bridge and the buffer functions are the same code in both builds. `mock::bridge_sent()` lists the envelopes the app sent, `mock::bridge_deliver(text)` plays the host (an answer, or an event) and `mock::bridge_unavailable()` plays a page that has none.

## Testing in a browser

`scripts/ui-web-test.fish` builds `examples/ui_web_app` for wasm, starts `examples/ui_web_server` (a `quench-http` server that sends a strict Content-Security-Policy), and runs `tests/browser.mjs`, which drives headless Chromium over the DevTools protocol (Node 22 and `chromium` needed). It checks that the app renders and fetches, that the counter patches the same text node, that submitting a form does not reload the page and clears the input, that keyed items keep their DOM nodes, that 100 items added and removed return the JS handle table to its starting size, that nothing was logged as an error, and that a panic shows the overlay and stops the app.

Unit tests (`tests/unit`) cover the renderer, handle release and reuse, 10,000 nodes coming and going, event decoding and flags, invalid UTF-8 and unknown handles or kinds, buffer allocation, the executor, timers and `fetch` including abort. Line coverage 96% on the host build; `host/wasm.rs` only compiles for `wasm32`.

Hydration is checked in the same run (see `docs/quench-fullstack.md`): the nodes the server sent are the nodes the page keeps, six kinds of damaged server HTML each end as the correct page with the mismatch reported in development and silent in production, and a page with nothing rendered is built from scratch.

Not covered: touch gestures and a real mobile `WebView` (no device was connected), and CSP rules for inline `style` attributes: `Element::style(..)` writes a `style` attribute, which a policy without `'unsafe-inline'` in `style-src-attr` blocks, so use classes or the stylesheet under a strict policy (Stage 5).

[Home](../README.md)
