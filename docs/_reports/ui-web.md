# Gap report: quench-ui-web and quench-ui-web-macros

## 1. Stale or wrong statements in the old docs (`quench-ui-web.md`, `quench-ui-web-macros.md`)

* Old page: "`app!` ... installs the panic hook, then builds the view". The code (`src/lib.rs` `run`) also installs `WebPlatform` through `quench_ui::platform::install`; not mentioned.
* Old page: "`quench.js` (under 400 lines)". It is 399 lines: correct but fragile.
* Old page: "`q_log` level 3 also shows an overlay on localhost, 127.0.0.1, [::1] or when `window.QUENCH_DEV` is true". Incomplete: `dev` can also be forced by `start(url, { dev })` and `?dev=1|0` on the script URL, and those take precedence (`js/quench.js` top of `start`, last lines).
* Old page: "`q_prevent` ... Called after `q_listen`" is true of the Rust call order but JS does not require it (the mode is looked up when the event fires).
* Old page said bridge answers "from any thread"; that is a statement about the host, not verifiable in this repo.
* Old page: the Rust API table omits `WebPlatform`, `BrowserHistory`, the `browser` module (`time_zone`, `wall_ms`, `storage_get/set`), `kind_code`/`decode_event`, `pending_tasks`/`pending_fetches`, `Sleep`, `AbortHandle`, the whole `plugins` module (11 clients, 22 data types) and `mock`'s API.
* Old page says `Request::header` drops names/values with a line break; the code also drops names containing `:` (`src/fetch.rs` `header`).
* Old macros page: "`Echo::NAME == "echo"`" is correct; "documentation and visibility are kept" is correct but it does not say the trait's attributes go on the struct and that a `#[derive(Clone, Copy, Debug)]` is added.
* Old pages and source comments refer to `docs/quench-ui-web.md`, now deleted: `quench-ui-web/src/lib.rs:4`, `src/host/wasm.rs:4`, `js/quench.js:2` and `docs/quench-android.md`/`quench-desktop.md` (links fixed). The source comments were not changed (docs-only rule); they should point at `docs/crates/quench-ui-web/internals.md`.
* `plugins.rs:4` says "what each method does is in `docs/quench-android.md`"; that page still exists.
* `docs/README.md` file tree and the root `README.md` index linked the deleted flat pages; links repointed.

## 2. Undocumented before, now documented

* The exact 30 imports and 11+ exports with argument lists, buffer ownership rules (`q_alloc` zero length returns 0; unknown address reads as empty data; `take` truncation).
* Boot order of `q_set_location`, `q_set_state`, `q_set_env`, `q_main?.()` (optional call) and the `instantiateStreaming` fallback.
* `q_history` treats every `op` other than 0, 1, 2 as forward (`js/quench.js` `go`).
* `Sleep` starts its timer at first poll; `WebPlatform::cancel` does not clear the browser timer (the sleeping task runs to the end).
* `EventSource::next` delivers `Closed` once, then `None`; `Request` GET/HEAD bodies are never sent.
* `BrowserHistory::back/forward` always return `true` and do not update the stored location until `popstate`; subscribers are never removed.
* `bridge::on_event`: single pump task, never restarted, events before the source opens are lost; `bridge::reset`, `browser::reset` are public on native targets.
* Bridge protocol details: 3 s connect wait, trusted-message check, only one bridge event source at a time (`bridgeEvents` holds the last opened id), desktop `webkit.messageHandlers.quench` path.
* Whole `quench_ui_web::mock` API (`Call`, `FetchCall`, 31 helpers) and what it does not simulate.
* The `#[plugin]` compile errors, return type rule, and that trait generics/supertraits/where clauses are silently dropped.

## 3. Open questions and suspected bugs

* `quench-ui-web-macros/src/expand.rs` (`method`, `keys` built from `Ident::to_string`): for a raw identifier parameter (`r#type`) the JSON key would be `r#type` including the prefix; `plugins::Incoming` has `r#type` as a field, so the same question applies to quench-serde derive. Not tested either way.
* `expand.rs` `plugin`: generics, supertraits, `where` clauses and `unsafe`/`auto` markers on the trait are ignored without error.
* `js/quench.js` `bridgeCall`: `JSON.parse(message)` of an invalid envelope throws inside `guarded`; the error is only a `console.warn` and the Rust fetch never completes (hangs forever). Rust always sends valid JSON, so it is theoretical.
* `js/quench.js` `bridgeAnswer`: `JSON.parse(data)` of a host message that is not JSON throws inside the message handler (uncaught in the port callback).
* `js/quench.js` `q_sse_open` for the bridge: a second `EventSource::open("quench-bridge:events")` overwrites `bridgeEvents`; closing the first source then does nothing (the `if (bridgeEvents === id)` guard) but the second keeps working. Intentional? Only one is supported.
* `src/exec.rs` `WebPlatform::after` (`src/platform.rs`): cancelled timers keep a pending task until the browser timer fires (visible in `pending_tasks()`); cheap but a leak for long timers.
* `src/fetch.rs` ids are a wrapping `u32` counter with no collision check against pending ids (needs 4 billion requests; theoretical).
* `src/lib.rs` doc comment on `app!`/`run`: `app!` expands `unsafe(no_mangle)` inside the user's crate; a crate with `#![forbid(unsafe_code)]` cannot use it on wasm32. Possibly intentional.
* `src/ffi.rs` `q_dispatch_event` is declared with `#[allow(clippy::must_use_candidate)]` and returns u32 flag; fine. `src/ffi.rs` `take`: an unknown nonzero address with `len > 0` silently reads as empty data, which hides double-use bugs.
* Wasm build profile (`wasm`) and the CSP statements could not be verified from this crate; see *Unverified* lines in the pages.
* A previous `git rm --cached` on `docs/quench-ui-web.md` was run by mistake while deleting the file (staging only; the file is deleted in the working tree as intended). The index entry for that file is therefore staged-as-deleted rather than unstaged; no other git state was changed.
