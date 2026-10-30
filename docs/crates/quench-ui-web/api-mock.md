---
title: API mock host
parent: quench-ui-web
grand_parent: Crates
nav_order: 4
---

`quench_ui_web::mock` (`src/host/mock.rs`, re-exported only on `cfg(not(target_arch = "wasm32"))`): an in-memory stand-in for `quench.js`. On `wasm32` the same `host` calls are the imports of `quench.js` (`src/host/wasm.rs`); everywhere else they are this module. It keeps a handle table with a free list in front of a DOM that outlives the handles (releasing a handle does not detach its node), records every call, and lets a test play the browser. State is thread-local; each test thread has its own. See [Testing](testing.md) for how to use it.

Event kinds, timers and ids are the raw numbers of the contract ([Internals](internals.md)).

## Types

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    CreateElement(u32, String), CreateText(u32, String), CreateAnchor(u32),
    SetText(u32, String), SetAttr(u32, String, String), RemoveAttr(u32, String),
    Insert { parent: u32, child: u32, before: u32 }, Remove { parent: u32, child: u32 },
    Listen(u32, u32), Release(u32),
    SetTimeout(u32, u32), ClearTimeout(u32),
    Fetch(u32), FetchAbort(u32),
    HydrateBegin, HydrateEnd,
    SseOpen(u32), SseClose(u32),
    Prevent(u32, u32, u32), SetStyle(u32, String, String),
    Focus(u32), TrapFocus(u32), History(u32, String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchCall { pub id: u32, pub method: String, pub url: String, pub headers: String, pub body: Vec<u8> }
```

`Call` is a call the Rust side made into the host, in order. `log`, `now`, `wall_ms`, `storage_set`, `prefers_reduced_motion`, `root_has_content` and `schedule_tick` are not recorded as `Call`s (logs, storage sets and ticks have their own accessors). `FetchCall.headers` is the raw `name: value` lines text.

## Functions

Mutating helpers are not `#[must_use]` (`clippy::must_use_candidate` is allowed for the module); getters are.

### State and DOM

| Function | Behaviour |
|---|---|
| `reset()` | Resets the runtime first (`reset_runtime`: DOM listeners, history, state, executor, fetches, SSE, bridge), then `browser::reset()`, then the host state to just the root. Call at the start of every test. Dropping what the last test left (an open `EventSource`) calls the host, so the order matters. |
| `calls() -> Vec<Call>` | The calls so far, oldest first. |
| `clear_calls()` | Empties that list. |
| `live_handles() -> usize` | Handles in use, not counting the root; 0 after a view is unmounted. |
| `live_listeners() -> usize` | Listeners the host has registered. |
| `handles_by_tag(tag: &str) -> Vec<u32>` | Handles of elements with this tag still in the table, ascending. |
| `attr(handle: u32, name: &str) -> Option<String>` | An attribute of the element behind a handle. |
| `html() -> String` | The root's content as HTML; dynamic-region anchors appear as `<!---->`; void elements have no end tag; text and attributes escaped. |
| `text() -> String` | All text under the root. |
| `style(handle: u32, property: &str) -> Option<String>` | The CSS property set through `q_set_style`. |

### Events

| Function | Behaviour |
|---|---|
| `has_listener(handle: u32, kind: u32) -> bool` | Whether the host has a listener of that kind on the handle. |
| `fire(handle: u32, kind: u32, payload: &str, a: f64, b: f64, c: i32, flags: u32) -> bool` | Delivers an event as `quench.js` would, through `q_dispatch_event`, if a listener of that kind is registered; returns whether one was. |
| `fire_click(handle: u32) -> bool` | `fire(handle, 0, "", 0.0, 0.0, 0, 0)`. |
| `fire_input(handle: u32, value: &str) -> bool` | `fire(handle, 2, value, ...)`. |

### Timers, ticks and fetches

| Function | Behaviour |
|---|---|
| `timers() -> Vec<(u32, u32)>` | Timers waiting: `(id, ms)`, ascending id. |
| `fire_timer(id: u32) -> bool` | Fires it (`q_timer_fire`), then `run_ticks()`. `false` if the id is unknown. |
| `fetches() -> Vec<FetchCall>` | Fetches started and not yet completed or aborted, oldest id first. Bridge calls are included. |
| `complete_fetch(id: u32, status: u32, body: &[u8], headers: &str) -> bool` | Completes it (`q_fetch_done`), then `run_ticks()`. `false` if unknown. Status 0 plus a body is an error message. |
| `run_ticks() -> bool` | Does what the browser does after a call returns: delivers the completions (status 0, body `aborted`) of aborted fetches and runs the tick asked for with `q_schedule_tick`, until nothing is left. Returns whether anything ran. Needed after `spawn` and after other actions that wake tasks. |
| `set_now(ms: f64)` | The value `q_now` reads (default 0.0). |
| `set_wall_ms(ms: i64)` | The value `q_wall_ms` reads (default 0). |
| `set_reduced_motion(on: bool)` | What `prefers-reduced-motion` reports (default false). |

### Server-sent events

| Function | Behaviour |
|---|---|
| `sse_sources() -> Vec<(u32, String, Vec<String>)>` | Sources the Rust side has open: `(id, url, event names)`. |
| `sse_emit(id: u32, kind: u32, name: &str, data: &str) -> bool` | `kind` 0 opened, 1 event, 2 reconnecting, 3 failed for good. An event (kind 1) is delivered only if the source is open and listens for that name (`message` always). Returns whether anything was delivered. |

### Bridge

| Function | Behaviour |
|---|---|
| `bridge_sent() -> Vec<String>` | The envelopes the app sent, oldest first (also in `fetches()` until answered). |
| `bridge_deliver(text: &str)` | Plays the host: first `run_ticks()` (the app may have just asked for its event source). A message with an `event` field is emitted on the `quench-bridge:events` source as an event named `event` (dropped if the app has none). A message with a numeric `id` completes the pending `quench-bridge:call` fetch with that envelope id (status 200, body = `text`). Anything else, or an answer to nothing, is ignored. |
| `bridge_unavailable()` | Plays a page with no host: completes every waiting `quench-bridge:call` with status 0 and the message `this page is not shown by a host app`. |

### Hydration, location, history, storage, logs

| Function | Behaviour |
|---|---|
| `set_server_content(present: bool, mismatches: u32)` | Pretends the root has server-rendered content (`run` then hydrates) and sets what `hydrate_end` reports. The claiming itself is done by `quench.js` and is not simulated; the browser test covers it. |
| `set_location(url: &str)` | Calls `q_set_location`, as `quench.js` does before `q_main`. |
| `popstate(url: &str)` | Calls `q_popstate`. |
| `history_entries() -> (Vec<String>, usize)` | The session history: entries and the current index. Initial: `(["/"], 0)`. Push truncates forward entries, adds one and moves; replace overwrites the current one; back/forward move if possible and report the entry as a `popstate` would. |
| `storage_sets() -> Vec<(String, String)>` | What the module asked the page to keep, in order; an empty value means forget. |
| `logs() -> Vec<(u32, String)>` | `(level, message)` of every `q_log`. |
| `alloc_copy(bytes: &[u8]) -> (usize, usize)` | Re-export of `ffi::alloc_copy`: a `q_alloc` buffer holding a copy, as `(address, length)`. |

Not simulated by the mock: `q_focus`/`q_trap_focus` effects (only recorded), `q_reduced_motion` beyond the flag, hydration claiming, `localStorage`, `Intl`, the 3 s connect timeout of the bridge, and `preventDefault` of real DOM events.

A private `imports` module (`pub(crate) mod imports`) holds the host functions the crate itself calls (`create_element`, `fetch`, ...); it is not API.
