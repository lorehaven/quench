---
title: API core
parent: quench-ui-web
grand_parent: Crates
nav_order: 3
---

Reference of the root items of `quench_ui_web`: starting an app, the DOM renderer, event decoding, the executor, history, the platform, the `browser` module and the exports `quench.js` calls. Network futures are in [API: net](api-net.md), the host bridge in [API: bridge](api-bridge.md), plugins in [API: plugins](api-plugins.md), the test host in [API: mock host](api-mock.md).

Module layout of `src/lib.rs`: `pub mod bridge; pub mod browser; pub mod plugins;` and the private modules `dom, events, exec, fetch, ffi, history, host, platform, sse, state`, whose public items are re-exported at the root. `extern crate self as quench_ui_web;` lets the `#[plugin]` expansion name `::quench_ui_web` inside this crate.

## Root items (`src/lib.rs`)

### `app!`

```rust
#[macro_export]
macro_rules! app {
    ($app:path) => { /* ... */ };
}
```

`quench_ui_web::app!(my_app);` where `my_app` is a path to a `fn() -> impl IntoView`. It expands to

```rust
#[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
pub extern "C" fn q_main() {
    $crate::run($app);
}
```

so `q_main` is an export only on `wasm32`; on other targets it is an ordinary public function (tests may call it). The expansion contains `unsafe(no_mangle)` inside `cfg_attr`; a crate that sets `#![forbid(unsafe_code)]` cannot use the macro on `wasm32`. The macro takes exactly one `path`; a closure is not accepted.

### `run`

```rust
pub fn run<V: IntoView>(app: impl FnOnce() -> V)
```

What `q_main` runs. In order: `install_panic_hook()`, `quench_ui::platform::install(Rc::new(WebPlatform::new()))`, then, if the host reports content in the root (`q_root_has_content`), `hydrate_root(app()).0.leak()`, else `mount_root(app()).leak()`. The mount is leaked on purpose: the page owns it until it closes. The hydration mismatch count is discarded by `run`.

### `mount_root`

```rust
pub fn mount_root(view: impl IntoView) -> Mount<DomRenderer>
```

Builds `view` into the root (`DomRenderer::root()`, handle 1) and returns the `quench_ui::Mount` to unmount later. Dropping the mount tears the view down; handles return to the free list (see [Testing](testing.md): `live_handles()` is back to 0).

### `hydrate_root`

```rust
pub fn hydrate_root(view: impl IntoView) -> (Mount<DomRenderer>, u32)
```

Builds `view` over the server-rendered content already in the root. Calls the host `q_hydrate_begin`, mounts, calls `q_hydrate_end`. Returns the mount and the number of nodes that did not match (0 when server and client agree). Every mismatch is repaired by building the client's node, so the page always ends as a client-only render would have made it. The algorithm is in [Internals](internals.md#hydration).

### `install_panic_hook`

```rust
pub fn install_panic_hook()
```

`std::panic::set_hook` with a hook that sends `info.to_string()` to `q_log` at level 3 (console `error`, and the overlay when `dev`). Called by `run`. Replaces any earlier hook.

### `QUENCH_JS`

```rust
pub const QUENCH_JS: &str = include_str!("../js/quench.js");
```

The source of `quench.js`, for a server to serve at the URL the page's `<script type="module">` names.

### `initial_state`

```rust
#[must_use]
pub fn initial_state() -> Option<String>
```

The text content of `<script type="application/json" id="__q_state">` if the page has one, set through `q_set_state` before `q_main`. Parse it with the types the server serialised (for example `quench_serde::json::from_str`). `None` when the element is absent. Thread-local: valid on the app's thread only.

### Re-exports

| Item | From |
|---|---|
| `BridgeError` | `bridge` |
| `DomRenderer`, `Node` | `dom` |
| `decode_event` (= `events::decode`), `kind_code` | `events` |
| `Sleep`, `pending_tasks`, `sleep`, `spawn` | `exec` |
| `AbortHandle`, `Fetch`, `FetchError`, `Request`, `Response`, `pending_fetches` | `fetch` |
| `q_alloc`, `q_dealloc`, `q_dispatch_event`, `q_fetch_done`, `q_popstate`, `q_set_env`, `q_set_location`, `q_set_state`, `q_sse_event`, `q_tick`, `q_timer_fire` | `ffi` |
| `BrowserHistory` | `history` |
| `mock` (only `cfg(not(target_arch = "wasm32"))`) | `host::mock` |
| `WebPlatform` | `platform` |
| `plugin` (attribute macro) | `quench-ui-web-macros` |
| `EventSource`, `SseEvent` | `sse` |

`__private` (`#[doc(hidden)] pub mod __private { pub use quench_serde::json; }`) is for the `#[plugin]` expansion only and is not API.

## `DomRenderer` and `Node` (`src/dom.rs`)

### `Node`

```rust
#[derive(Clone)]
pub struct Node(/* Rc<Handle> */);
impl Node {
    #[must_use]
    pub fn id(&self) -> u32;
}
impl std::fmt::Debug for Node   // prints "Node(<id>)"
```

A DOM node known to `quench.js` by a `u32` handle. Cloning is cheap (`Rc`). When the last clone drops, the node's listeners are removed from the Rust table and `q_release(handle)` is called (JS frees the slot and its listeners). The root node returned by `DomRenderer::root()` is not owned: dropping it releases nothing. `Node` is `!Send`/`!Sync`.

`id()` is the handle number as `quench.js` knows it.

### `DomRenderer`

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct DomRenderer;

impl DomRenderer {
    #[must_use]
    pub fn root() -> Node;
}
```

`root()` is handle 1: the element with id `app`, or `document.body` if there is none.

`impl quench_ui::Renderer for DomRenderer` with `type Node = Node`:

| Method | Host call |
|---|---|
| `create_element(&self, tag: &str) -> Node` | `q_create_el` |
| `create_text(&self, text: &str) -> Node` | `q_create_text` |
| `create_anchor(&self) -> Node` | `q_create_anchor` (an empty comment node, the marker of a dynamic region) |
| `set_text(&self, node, text)` | `q_set_text` (`node.data = text`) |
| `set_attr(&self, node, name, value)` | `q_set_attr` |
| `remove_attr(&self, node, name)` | `q_remove_attr` |
| `insert(&self, parent, child, before: Option<&Node>)` | `q_insert`; `None` is sent as handle `0` (append) |
| `remove(&self, parent, child)` | `q_remove` |
| `listen(&self, node, kind: EventKind, handler: Handler)` | stores the handler per handle; calls `q_listen(handle, kind_code(kind))` only for the first listener of that kind on the node |
| `prevent_default(&self, node, kind, how: Prevent)` | `q_prevent(handle, kind_code, mode)`; `Prevent::Always` is mode 1, `Prevent::PlainClick` is mode 2 |
| `set_style(&self, node, property, value)` | `q_set_style` (an empty value removes the property) |
| `focus(&self, node)` | `q_focus` |
| `trap_focus(&self, node)` | `q_trap_focus` |

An attribute named `value`, `checked` or `selected` also sets the DOM property on the JS side, so `attr_fn("value", ..)` keeps an input in step after the user typed.

Incoming events are routed by the private `dom::dispatch(handle, &Event)`: it clones the handlers registered for that handle whose kind equals `event.kind()` and runs them with no borrow held, so handlers may change the DOM and add listeners. An unknown handle is ignored.

## Events (`src/events.rs`)

```rust
#[must_use]
pub fn kind_code(kind: EventKind) -> u32

#[must_use]
pub fn decode(kind: u32, payload: String, a: f64, b: f64, c: i32, flags: u32) -> Option<Event>   // re-exported as decode_event
```

`kind_code` returns the table index of the listener kind (table below). It panics (`expect("every kind is in the table")`) only if `quench_ui::EventKind` gains a variant missing from the table.

`decode` builds the typed `quench_ui::Event`; `None` for a `kind` outside 0..=17. `payload` is the text (value or key); `(a, b, c)` are a pointer's `x`, `y`, `pointer_id`, a scroll's `top`, `left`, or a resize's `width`, `height`; `flags` are modifier bits: 1 ctrl, 2 shift, 4 alt, 8 meta.

| kind | `EventKind` | Event built | Data used |
|---|---|---|---|
| 0 | `Click` | `Event::Click` | none |
| 1 | `DoubleClick` | `Event::DoubleClick` | none |
| 2 | `Input` | `Event::Input(String)` | payload |
| 3 | `Change` | `Event::Change(String)` | payload |
| 4 | `KeyDown` | `Event::KeyDown(KeyEvent)` | payload (`key`), flags |
| 5 | `KeyUp` | `Event::KeyUp(KeyEvent)` | payload, flags |
| 6 | `Submit` | `Event::Submit` | none |
| 7 | `Focus` | `Event::Focus` | none |
| 8 | `Blur` | `Event::Blur` | none |
| 9 | `PointerDown` | `Event::PointerDown(PointerEvent { x: a, y: b, pointer_id: c })` | a, b, c |
| 10 | `PointerUp` | `Event::PointerUp(PointerEvent)` | a, b, c |
| 11 | `PointerMove` | `Event::PointerMove(PointerEvent)` | a, b, c |
| 12 | `CompositionStart` | `Event::CompositionStart(String)` | payload |
| 13 | `CompositionEnd` | `Event::CompositionEnd(String)` | payload |
| 14 | `Scroll` | `Event::Scroll(ScrollEvent { top: a, left: b })` | a, b |
| 15 | `TransitionEnd` | `Event::TransitionEnd` | none |
| 16 | `Resize` | `Event::Resize(Size { width: a, height: b })` | a, b |
| 17 | `PointerCancel` | `Event::PointerCancel(PointerEvent)` | a, b, c |

## Executor and timers (`src/exec.rs`)

The wasm module has no threads or runtime. `spawn` queues a task; the first queued task asks the host for one tick (`q_schedule_tick`, which JS turns into `queueMicrotask`); `q_tick` polls every ready task, including tasks that become ready while polling. Many spawns before a tick ask for one tick.

```rust
pub fn spawn(future: impl Future<Output = ()> + 'static)
```

Runs `future` to completion in the background. It is polled for the first time at the next tick, not inside `spawn`. The future need not be `Send`. Wakers are `Send + Sync` (they hold only a task id) but waking from another thread would touch that thread's own tables; the browser has one thread.

```rust
#[must_use]
pub fn pending_tasks() -> usize
```

Tasks spawned and not finished.

```rust
#[must_use]
pub const fn sleep(ms: u32) -> Sleep

#[derive(Debug)]
pub struct Sleep { /* ms, id, done */ }
impl Future for Sleep { type Output = (); }
```

Completes after `ms` milliseconds. The timer (`q_set_timeout`) starts at the **first poll**, not at creation. Dropping a `Sleep` before it fires clears the timer (`q_clear_timeout`). `sleep(0)` still goes through the host and completes at a later tick.

Private but observable: `exec::tick` (the body of `q_tick`) clears the "tick scheduled" flag, then loops over the ready queue. A task finished by a poll is dropped outside the executor borrow.

## History (`src/history.rs`)

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserHistory;
impl BrowserHistory {
    #[must_use]
    pub const fn new() -> Self;
}
impl quench_ui::router::History for BrowserHistory { /* ... */ }
```

The router's `History` over the browser's session history. The current location is a thread-local initialised to `Location::parse("/")`, set by `q_set_location` before `q_main` and updated by `q_popstate`.

| Method | Behaviour |
|---|---|
| `location(&self) -> Location` | the stored current location |
| `push(&self, &Location)` | stores it, then `q_history(0, url)` (JS saves the scroll of the entry being left, `pushState`, scrolls to the top) |
| `replace(&self, &Location)` | stores it, then `q_history(1, url)` (`replaceState`) |
| `back(&self) -> bool` | `q_history(2, "")` (`history.back()`); always returns `true`; the stored location changes only when `popstate` arrives |
| `forward(&self) -> bool` | `q_history(3, "")`; always `true`; same |
| `subscribe(&self, Rc<dyn Fn(Location)>)` | adds a listener, called with the new location on every `q_popstate`; listeners are never removed |

`url` is `Location::url()` of `quench-ui`. Scroll positions are saved and restored by `quench.js` (`history.scrollRestoration = "manual"`).

## Platform (`src/platform.rs`)

```rust
#[derive(Debug, Default)]
pub struct WebPlatform { /* next: Cell<u32>, live: Rc<RefCell<HashMap<u32, Rc<Cell<bool>>>>> */ }
impl WebPlatform {
    #[must_use]
    pub fn new() -> Self;
}
impl quench_ui::platform::Platform for WebPlatform { /* ... */ }
```

Installed by `run` through `quench_ui::platform::install`.

| Method | Behaviour |
|---|---|
| `now(&self) -> f64` | `q_now()`, `performance.now()` in ms (starts at 0 with the page) |
| `after(&self, ms: u32, f: Box<dyn FnOnce()>) -> u32` | ids start at 1; spawns a task that sleeps `ms`, then runs `f` unless cancelled |
| `cancel(&self, id: u32)` | marks the timer cancelled; the underlying `sleep` task still runs until the browser timer fires, then does nothing |
| `prefers_reduced_motion(&self) -> bool` | `q_reduced_motion() != 0` |

## `browser` module (`src/browser.rs`, `pub mod browser`)

What the page knows that the module cannot ask the DOM for.

```rust
pub fn set_env(json: &str)
#[must_use] pub fn wall_ms() -> i64
#[must_use] pub fn time_zone() -> String
#[must_use] pub fn storage_get(key: &str) -> Option<String>
pub fn storage_set(key: &str, value: Option<&str>)
#[cfg(not(target_arch = "wasm32"))] pub fn reset()
```

* `set_env(json)`: takes `{"time_zone": "...", "storage": {"key": "value"}}` (what `q_set_env` receives). Text that does not parse as JSON is ignored (state unchanged). A missing or non-string `time_zone` leaves none; `storage` values that are not strings are skipped. It replaces the whole state.
* `wall_ms()`: milliseconds since 1970-01-01T00:00:00Z (`Date.now()`); `q_wall_ms` returns an `f64` truncated with `as i64`.
* `time_zone()`: the IANA zone from `Intl.DateTimeFormat().resolvedOptions().timeZone`, `"UTC"` if the page did not say (before `q_set_env`, or the `Intl` value is empty).
* `storage_get(key)`: the value from the in-memory copy (filled at start from `localStorage` keys that begin with `q:`, minus the prefix).
* `storage_set(key, value)`: `Some(v)` with a non-empty `v` stores it in memory and calls `q_storage_set` (JS writes `localStorage["q:" + key]`). `None` or `Some("")` forgets it (JS `removeItem`). If the browser refuses (blocked storage) JS ignores the error and the value lives in memory until the page closes.
* `reset()`: non-wasm only; clears the state. Public in the module on native targets.

## Exports (`src/ffi.rs`)

These are the functions `quench.js` calls on the module. All are `pub extern "C"` and, on `wasm32`, `#[unsafe(no_mangle)]` exports. On native targets they are plain functions (the mock host calls them). Buffer rules are in [Internals](internals.md#strings-and-buffers). In every function below, an `(address, len)` pair is a buffer obtained from `q_alloc` and is consumed.

```rust
#[must_use]
pub extern "C" fn q_alloc(len: usize) -> usize
pub extern "C" fn q_dealloc(address: usize, len: usize)
#[allow(clippy::too_many_arguments, clippy::must_use_candidate)]
pub extern "C" fn q_dispatch_event(handle: u32, kind: u32, address: usize, len: usize, a: f64, b: f64, c: i32, flags: u32) -> u32
pub extern "C" fn q_set_state(address: usize, len: usize)
pub extern "C" fn q_set_env(address: usize, len: usize)
pub extern "C" fn q_set_location(address: usize, len: usize)
pub extern "C" fn q_popstate(address: usize, len: usize)
pub extern "C" fn q_sse_event(id: u32, kind: u32, name_address: usize, name_len: usize, data_address: usize, data_len: usize)
pub extern "C" fn q_tick()
pub extern "C" fn q_timer_fire(id: u32)
pub extern "C" fn q_fetch_done(id: u32, status: u32, body_address: usize, body_len: usize, headers_address: usize, headers_len: usize)
```

| Export | Behaviour |
|---|---|
| `q_alloc(len)` | Returns 0 for `len == 0`. Otherwise allocates a zeroed `Vec<u8>` of `len` bytes, registers it by its address and returns the address. The export the address is passed to takes ownership. |
| `q_dealloc(address, len)` | Frees a buffer that was not handed to an export. An unknown address is ignored. |
| `q_dispatch_event(...)` | Takes the payload buffer, decodes it with lossy UTF-8 (invalid bytes become U+FFFD), clears any stale prevent request, builds the event with `decode`, dispatches to the handle's listeners, and returns `1` if a handler asked for `preventDefault` (`quench_ui::prevent_default()`), else `0`. An unknown kind builds no event. |
| `q_set_state` | `initial_state()` source. Lossy UTF-8. |
| `q_set_env` | `browser::set_env`. |
| `q_set_location` | Sets the history's initial location (`Location::parse`). |
| `q_popstate` | Updates the current location and calls all history subscribers. |
| `q_sse_event` | Kinds 0 open, 1 message, 2 reconnecting, 3 closed; other kinds are ignored. Both buffers are consumed; the name and data are lossy UTF-8. |
| `q_tick` | `exec::tick()`. |
| `q_timer_fire(id)` | Wakes the `Sleep` waiting on that timer. |
| `q_fetch_done` | `status` 0 means network error or abort and the body is the message (UTF-8 lossy); otherwise the HTTP status (clamped to `u16::MAX`), body bytes and `name: value` header lines. The completion of a request nobody waits for is ignored. |

A buffer whose `len` is 0, or whose address was never allocated (or already consumed), reads as empty data.

`q_main` is not defined by this crate: `app!` defines it.

Non-wasm only: `ffi::alloc_copy(bytes: &[u8]) -> (usize, usize)` (`#[must_use]`) allocates a `q_alloc` buffer holding a copy of `bytes`; it is reachable as `quench_ui_web::mock::alloc_copy`.
