---
title: Internals
parent: quench-ui-web
grand_parent: Crates
nav_order: 8
---

The exact contract between the wasm module (Rust) and `quench.js`. Changing either side means changing this page. Everything here was checked against `js/quench.js`, `src/ffi.rs`, `src/host/wasm.rs` and the Rust modules.

## Boot sequence

1. `quench.js` evaluates; if its URL has `?wasm=`, it calls `start(url)`.
2. `start` computes `dev`, builds the handle table (`nodes = [null, root]`: handle 0 is "none", handle 1 the root), and defines the `env` import object.
3. It instantiates the module with `WebAssembly.instantiateStreaming(fetch(url), { env })`; if that throws (a server that does not send `application/wasm`) it falls back to `WebAssembly.instantiate(await (await fetch(url)).arrayBuffer(), { env })`.
4. `performance.mark("quench:instantiated")`.
5. Exports, in this order, each through a guarded `call`: `q_set_location(here())` where `here()` is `pathname + search + hash`; `q_set_state(textContent of #__q_state)` if that element exists; `q_set_env(JSON)` with `{"time_zone": <Intl zone or "UTC">, "storage": {key: value}}` built from the `localStorage` keys that start with `q:` (prefix removed; `{}` if storage throws); then `q_main?.()` (the optional call means a module without `q_main` boots silently).
6. `performance.mark("quench:ready")`; returns the API object.

## Handles

A node is a `u32` handle into a JS array with a free list. `0` is "none" and never valid; `1` is the root and is never released (`release` ignores `h < 2`).

* Every handle returned by `q_create_*` belongs to Rust until Rust calls `q_release(h)` exactly once. On the Rust side a handle is an `Rc` whose last drop calls `q_release`.
* `q_release` removes the listeners and `ResizeObserver` the JS side added for that handle and its `prevent` entries, nulls the slot and pushes the handle on the free list; a later `q_create_*` reuses it (most recently freed first).
* A handle that is out of range, already released, or of the wrong node type is ignored by every import (no exception).

## Strings and buffers

All text is UTF-8 in linear memory.

* **Rust to JS** (`ptr`, `len` of an import): borrowed for the duration of the call. On the Rust side the address is `str::as_ptr().addr()` cast to a number; nothing is dereferenced, so the imports are declared `safe fn`. JS copies (decodes) before returning and re-creates its `Uint8Array` on `memory.buffer` for every call, because memory may grow. `len == 0` reads as the empty string.
* **JS to Rust** (`ptr`, `len` of an export): JS asks `ptr = q_alloc(len)`, writes `len` bytes at `ptr`, and passes `(ptr, len)` to **one** export, which takes ownership. `q_alloc(0)` returns `0` and needs no write. If JS decides not to call an export it calls `q_dealloc(ptr, len)`. Buffers are ordinary `Vec<u8>`s kept in a thread-local `BTreeMap<usize, Vec<u8>>` keyed by their address.
* Taking a buffer: `len == 0` removes the address (if any) and gives empty data; an unknown address gives empty data; otherwise the vector is truncated to `len`.
* Decoding is lossy UTF-8 (U+FFFD); never a panic. Fetch bodies stay bytes.

## Imports (Rust calls JS), module `"env"`

Declared in `src/host/wasm.rs` (`#[link(wasm_import_module = "env")] unsafe extern "C"`, all `safe fn`) and implemented by the `env` object in `quench.js`. Every import except `q_now`, `q_wall_ms` and `q_reduced_motion` is wrapped by `guarded` (below).

| Import | JS behaviour |
|---|---|
| `q_create_el(tag_ptr, tag_len) -> u32` | `document.createElement(tag)`; when hydrating, claims (see below). |
| `q_create_text(ptr, len) -> u32` | `document.createTextNode`. |
| `q_create_anchor() -> u32` | `document.createComment("")`. |
| `q_set_text(h, ptr, len)` | `node.data = text` (text and comment nodes). |
| `q_set_attr(h, name_ptr, name_len, val_ptr, val_len)` | Element nodes only. When hydrating and the attribute already has the value: nothing. When hydrating a claimed node with a different value: counts a mismatch. Then `setAttribute`; and for `value`, `checked`, `selected` also the property (`value` set unless it already equals; `checked`/`selected` set `true`). |
| `q_remove_attr(h, name_ptr, name_len)` | `removeAttribute`; for the three properties resets them (`value` to `""`, the others to `false`). |
| `q_insert(parent, child, before)` | `parent.insertBefore(child, before)`; `before == 0` appends (`get(0)` is null). An attached child moves. While hydrating see below. |
| `q_remove(parent, child)` | `node.remove()` if its `parentNode` is `parent`. |
| `q_listen(h, kind)` | Adds one DOM listener of the event type for `kind` (table below); a second call for the same type on the same handle is a no-op. Kind 16 (`resize`) uses a `ResizeObserver`. |
| `q_release(h)` | see Handles. |
| `q_log(level, ptr, len)` | `console[debug|info|warn|error]("quench: " + message)` for levels 0..3 (an unknown level logs as `error`); level 3 also shows the overlay when `dev`. The Rust panic hook uses level 3. |
| `q_set_timeout(id, ms)` | `setTimeout`; on fire, deletes the entry and calls `q_timer_fire(id)`. |
| `q_clear_timeout(id)` | `clearTimeout`. |
| `q_fetch(id, method_ptr, method_len, url_ptr, url_len, headers_ptr, headers_len, body_ptr, body_len)` | URL starting `quench-bridge:` is a bridge call (below). Else `fetch(url, { method, headers, body?, signal })`: headers parsed from `name: value` lines (split at the first `:`, trimmed; lines with no `:` or a leading `:` skipped); body sent only if `body_len > 0` and the method is not `GET`/`HEAD`. On completion `q_fetch_done(id, status, body, headers)` with headers `name: value` joined by `\n` from `response.headers`. On rejection `q_fetch_done(id, 0, message)` where the message is `"aborted"` for `AbortError` and `String(error)` otherwise. |
| `q_fetch_abort(id)` | `AbortController.abort()` (for a bridge call: cancels the wait, clears its timer, completes with status 0 and `aborted`). |
| `q_schedule_tick()` | `queueMicrotask(() => q_tick())`. |
| `q_sse_open(id, url_ptr, url_len, names_ptr, names_len)` | URL starting `quench-bridge:` registers the bridge event channel and reports `Open` at once. Else `new EventSource(url)` with a listener for `message` and each name (names separated by `\n`), reporting through `q_sse_event`: `onopen` kind 0; `onerror` kind 2 or, if `readyState` is `CLOSED`, kind 3 and the source is forgotten. |
| `q_sse_close(id)` | `EventSource.close()` and forget; nothing is reported afterwards (`sse` ignores unknown ids). |
| `q_root_has_content() -> u32` | `1` if `root.firstChild`. |
| `q_hydrate_begin()` | Starts hydration. |
| `q_hydrate_end() -> u32` | Trims unclaimed server nodes from the root frame, ends hydration, clears the origin table, returns the mismatch count. |
| `q_prevent(h, kind, mode)` | Stores `mode` for `"<h>:<type>"`: `1` always `preventDefault`; `2` links: a plain primary click (button 0 without ctrl/shift/alt/meta) is handled and prevented, any other click is neither handled nor prevented. Called after `q_listen`. |
| `q_set_style(h, name_ptr, name_len, val_ptr, val_len)` | `style.setProperty`; an empty value `removeProperty`. |
| `q_now() -> f64` | `performance.now()`. |
| `q_wall_ms() -> f64` | `Date.now()`. |
| `q_storage_set(key_ptr, key_len, val_ptr, val_len)` | `localStorage.setItem("q:" + key, value)`; an empty value `removeItem`; exceptions ignored. |
| `q_reduced_motion() -> u32` | `matchMedia("(prefers-reduced-motion: reduce)").matches`. |
| `q_focus(h)` | `queueMicrotask(() => node.focus())` (after attach). |
| `q_trap_focus(h)` | Adds a `keydown` listener on the node: Tab/Shift+Tab move among `a[href],button,input,select,textarea,[tabindex]:not([tabindex='-1'])` that are not disabled and have client rects, wrapping; with none inside, Tab is prevented. Removed on release. |
| `q_history(op, ptr, len)` | `0` save scroll, `pushState({}, "", url)`, `scrollTo(0,0)`; `1` `replaceState`; `2` `history.back()`; `3` `history.forward()` (the code treats any `op` other than 0, 1, 2 as forward). |

Only these `env` imports exist: the table is the complete list of 30.

## Exports (JS calls Rust)

`memory` (the instance's linear memory, used by JS as `ex.memory.buffer`), `q_main` (defined by `app!`), and the `q_*` functions documented in [API: core](api-core.md#exports-srcffirs). Default actions:

* **q_dispatch_event** returns `1` when a handler called `quench_ui::prevent_default()`; JS then calls `event.preventDefault()` (when delivery was not deferred).
* `submit` events are always `preventDefault`ed in JS (`mode` defaults to 1 for `submit`), so a form never reloads the page. Nothing else is prevented unless `q_prevent` asks.

### Event types per kind

`EVENTS = ["click","dblclick","input","change","keydown","keyup","submit","focus","blur","pointerdown","pointerup","pointermove","compositionstart","compositionend","scroll","transitionend","resize","pointercancel"]`, index = kind.

| Kinds | Payload `ptr/len` | `a`, `b`, `c` | `flags` |
|---|---|---|---|
| `input`, `change` | `String(currentTarget.value ?? "")` | 0 | 0 |
| `compositionstart/end` | `currentTarget.value ?? event.data ?? ""` | 0 | 0 |
| `scroll` | empty | `a = scrollTop`, `b = scrollLeft` of the element | 0 |
| `keydown`, `keyup` | `event.key` | 0 | bit 1 ctrl, 2 shift, 4 alt, 8 meta |
| `pointer*` | empty | `clientX`, `clientY`, `pointerId` | 0 |
| `resize` | empty | content width, height (ResizeObserver; reported at first layout and on change) | 0 |
| others | empty | 0 | 0 |

Touch input arrives as pointer events; scrolling and zooming are controlled with CSS `touch-action`.

### Delivery and re-entrancy

`guarded(fn)` wraps an import: it increments `busy`, runs `fn` (an exception is caught and logged with `console.warn("quench:", error)`, and the import returns `undefined`), and when `busy` returns to 0 delivers the events queued meanwhile. `dispatch` queues an event if `busy > 0` (returning 0 to the DOM listener), else delivers it at once. So an event raised inside an import (removing the focused input fires `blur`) is delivered right after the import returns, never inside it, and listeners cannot re-enter a half-finished update. Deferred events cannot honour `preventDefault` from the handler.

`call(fn)` runs an export; if it throws, the instance is marked `dead` (Rust traps with `panic = "abort"`), the error is logged once with `console.error("quench: the app stopped:", error)`, the overlay is shown in `dev`, and every later export call is skipped silently.

## Hydration

Between `q_hydrate_begin` and `q_hydrate_end`, creating a node claims the next node of the server-rendered DOM instead of making one. This works because the mount engine creates nodes in document order (an element before its children; a dynamic or keyed region's content before its anchor).

`quench.js` keeps a stack of frames `{ parent, cursor, fresh }` and an `origin` map (`claimed | replace | insert`) for each new handle until it is inserted:

* `q_create_el(tag)`: if the cursor is an element with that local name it is claimed and a frame is pushed for its children; otherwise a fresh element is made (mismatch) and a `fresh` frame is pushed (inside a fresh subtree nothing is claimed).
* `q_create_text(s)`: an empty `s` makes an empty text node marked `insert`. If the cursor is a text node longer than `s` that starts with `s`, it is split (`splitText`), since the HTML parser merges adjacent text. Otherwise a text node with data `s` is claimed, else replaced.
* `q_create_anchor()`: claims a comment with empty data (`<!---->`).
* `q_insert(parent, child, before)`: a claimed node is already in place; for an element, its frame is popped first and unclaimed server nodes left in it are removed (each reported as a mismatch). A fresh node is inserted at the cursor, and a `replace` node removes the server node it replaced.
* `q_set_attr`: see the import table.
* `q_hydrate_end`: removes server nodes left in the root frame.

Mismatches are logged in `dev` only: `quench: hydration mismatch in #app > div:nth-child(2): expected <h2>, found <h3>; rebuilt it`. What the server must write (`quench-ui`'s serialiser does all of it): elements with attributes in the order the view sets them, text exactly as the view has it, an empty comment for each dynamic or keyed region, a doubled newline after `<pre>`/`<textarea>` when the text starts with one, and no content in void elements.

## Async

`spawn` pushes a task and asks for one tick; `q_tick` polls every ready task. `Sleep` and `Fetch` futures are completed by `q_timer_fire` and `q_fetch_done`. Task ids are `u64`, timer/fetch/SSE ids wrapping `u32`. `Wake` for a task is a `TaskWaker(u64)`; a woken task not already queued is pushed on the ready queue and a tick is requested unless one is scheduled or running.

## Scroll and history

`history.scrollRestoration = "manual"`. Scroll `[scrollX, scrollY]` is saved into `history.state.scroll` on `scrollend` and `pagehide`, and before a push. On `popstate`, JS calls `q_popstate(here())` then `scrollTo` the entry's saved scroll (`[0, 0]` if none).

## The host bridge

A host app that shows the bundle in a WebView offers native plugins. The module reaches them through imports it already has: there is no bridge-specific function.

* **Calls are fetches.** `q_fetch` with URL `quench-bridge:call` and the JSON envelope as the body. If the user agent does not match `/QuenchHost\//` (`hosted` false), the fetch fails at once (`q_fetch_done(id, 0, "this page is not shown by a host app")`). Else the envelope is parsed for its `id`; the call is registered under that id and posted on the port (`MessagePort.postMessage(text)`) or, if the port has not arrived, queued with a 3 s timer after which it fails with `the host did not connect`. The fetch completes with status 200 and the answer text as the body when a message with that `id` arrives. An unparseable envelope makes the import throw (caught by `guarded`; the fetch never completes).
* **Events are an event source.** `q_sse_open` with `quench-bridge:events` records the source id and reports `Open`. Each host message with an `event` field is delivered as `q_sse_event(id, 1, "event", <message text>)`. Only one source is registered at a time (the last opened); closing it stops delivery.
* **Detection.** The host appends `QuenchHost/1` to the user agent.
* **Connecting (Android).** The host posts the message `"quench-bridge"` with one port to the window. `quench.js` accepts it only from an event that `isTrusted`, has `source === null`, `origin === ""`, `data === "quench-bridge"` and a port: that is how a native post looks and a script's `postMessage` does not. The previous port is closed, the new one's `onmessage` is the answer handler, and queued calls are flushed.
* **Desktop.** If `webkit.messageHandlers.quench` exists, the "port" is `{ postMessage: m => handler.postMessage(m), close() {} }` and `window.__quenchBridge(text)` is defined read-only; the host answers by calling it.
* **Answers.** Non-string messages are ignored. A message with an `event` key goes to the event channel; otherwise its `id` finds the waiting call. Answers may come in any order.
* **Envelope.**

```text
app → host   {"id":7,"plugin":"echo","method":"say","args":{"text":"hi"}}
host → app   {"id":7,"ok":"hi"}
host → app   {"id":7,"err":{"code":"permission_denied","message":"..."}}
host → app   {"event":"progress","plugin":"files","data":{..}}
```

Rust side: [API: bridge](api-bridge.md).

## Panics

`panic = "abort"` is expected in the release profile. A panic first runs the hook (`q_log(3, ..)`); the instance then traps; later export calls throw and `quench.js` has already marked itself `dead`.

## Thread-safety

No `Send`/`Sync`. All tables are `thread_local!`. `TaskWaker` is `Send + Sync` only because it holds an id.
