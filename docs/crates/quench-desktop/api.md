---
title: API reference
parent: quench-desktop
grand_parent: Crates
nav_order: 2
---

Root re-exports (`src/lib.rs`): `Answer`; `Bridge`, `Call`, `Deliver`, `Emitter`, `MAX_MESSAGE`, `Plugin`, `string_arg`; `Bundle`, `HOST`, `SCHEME`, `content_type`; `is_app_origin`, `opens_outside`, `origin_of`; with feature `window`: `Gtk`, `Remote`, `WebView`, `Window`, `App`, `RunError`, `run`. Public modules: `plugins`, and with `window`: `control`. Plugin types are in [plugins](plugins.md#api).

## `answer`

### `struct Answer`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer { pub status: u16, pub content_type: String, pub body: Vec<u8> }
```

What an asset request is answered with.

* `#[must_use] pub fn ok(content_type: &str, body: Vec<u8>) -> Self`: status 200.
* `#[must_use] pub fn not_found() -> Self`: status 404, `text/plain; charset=utf-8`, body `not found`.

## `bundle`

### Constants

```rust
pub const SCHEME: &str = "quench";
pub const HOST: &str = "app";
```

The app is served from `quench://app/`.

### `struct Bundle`

```rust
#[derive(Debug, Default, Clone)]
pub struct Bundle { /* BTreeMap<String, Cow<'static, [u8]>> */ }
```

The files of an app by path (`index.html`, `_quench/app.<hash>.wasm`).

* `#[must_use] pub fn new() -> Self`.
* `#[must_use] pub fn with(self, path: &str, bytes: impl Into<Cow<'static, [u8]>>) -> Self`: adds a file; leading slashes are stripped; embedded bytes are not copied.
* `pub fn from_dir(dir: &Path) -> io::Result<Self>`: reads every file under `dir` recursively (paths with `/`); errors when a directory or file cannot be read.
* `#[must_use] pub fn len(&self) -> usize`, `#[must_use] pub fn is_empty(&self) -> bool`.
* `#[must_use] pub fn get(&self, path: &str) -> Option<&[u8]>`.
* `#[must_use] pub fn answer(&self, uri: &str) -> Answer`: the response to a request for `uri`. Rules: the scheme must be `quench` and the host `app` (else 404); the query and fragment are cut; a path containing `\`, `%2f`, `%5c` (any case), or a `..` or `.` segment is 404; an empty path, or one whose last segment has no `.`, gets `index.html` (404 if the bundle has none); any other path is looked up (404 if absent). Content types as `content_type`.

### `content_type`

```rust
#[must_use]
pub fn content_type(path: &str) -> &'static str
```

By extension: `html` -> `text/html; charset=utf-8`; `js`/`mjs` -> `text/javascript; charset=utf-8`; `css` -> `text/css; charset=utf-8`; `wasm` -> `application/wasm`; `json` -> `application/json`; `svg` -> `image/svg+xml`; `png` -> `image/png`; `jpg`/`jpeg` -> `image/jpeg`; `ico` -> `image/x-icon`; `woff2` -> `font/woff2`; `txt` -> `text/plain; charset=utf-8`; else `application/octet-stream`.

## `origin`

```rust
#[must_use] pub fn origin_of(uri: &str) -> String
#[must_use] pub fn is_app_origin(uri: &str, dev_url: Option<&str>) -> bool
#[must_use] pub fn opens_outside(uri: &str) -> bool
```

* `origin_of`: `scheme://authority` of a URI (the authority runs to the first `/`, `?` or `#`); the empty string if there is no `://`.
* `is_app_origin`: true if `origin_of(uri)` is `quench://app` or equals the origin of `dev_url`.
* `opens_outside`: true if `uri` starts with `https://`, `http://` or `mailto:`.

## `bridge`

The page's way to the plugins: the same envelopes as the Android host. A request is `{"id","plugin","method","args"}`; the answer `{"id","ok"}` or `{"id","err":{"code","message"}}`; events `{"event","plugin","data"}`.

### `MAX_MESSAGE`

```rust
pub const MAX_MESSAGE: usize = 1 << 20;
```

The largest message taken; longer ones are dropped.

### `type Deliver`

```rust
pub type Deliver = Arc<dyn Fn(String) + std::marker::Send + Sync>;
```

Delivers a JSON text to the page.

### `struct Call`

The answer to one call. `Debug` prints the id.

* `pub fn resolve(self, value: &Value)`: sends `{"id", "ok": value}`.
* `pub fn reject(self, code: &str, message: &str)`: sends `{"id", "err": {"code","message"}}`.

Both consume the call: it is answered once. (Unlike the Android `Call`, there is no protection against answering twice because the type cannot be used twice.)

### `struct Emitter`

`#[derive(Clone)]`. `pub fn emit(&self, plugin: &str, event: &str, data: &Value)` pushes an event to the page.

### `trait Plugin`

```rust
pub trait Plugin: std::marker::Send + Sync {
    fn name(&self) -> &'static str;
    fn call(&self, method: &str, args: &Value, call: Call);
    fn attach(&self, _emitter: Emitter) {}
}
```

`call` receives the named arguments as a JSON object (an empty object if the page sent none or a non-object) and answers through `call`, now or later, from any thread. `attach` is called once, when the `Bridge` is built.

### `struct Bridge`

The plugins of an app and the way to the page. `Debug` lists the plugin names.

* `pub fn new(plugins: Vec<Arc<dyn Plugin>>, send: Deliver) -> Self`: indexes plugins by name (a later plugin with the same name replaces the earlier) and calls `attach` on each.
* `#[must_use] pub fn emitter(&self) -> Emitter`.
* `pub fn handle(&self, text: &str)`: handles one message from the page. Dropped silently: longer than `MAX_MESSAGE`, not JSON, or without a non-null `id` (nobody to answer). Unknown plugin: rejects `unknown_plugin` with ``no plugin `<name>` ``. A panic in the plugin is caught (`catch_unwind`): `quench-desktop: <name>.<method> panicked` on standard error and a rejection `internal` with `<name>.<method> failed`. (If the plugin had already answered before panicking, a second reply is sent; the page ignores answers to unknown ids.)

### `string_arg`

```rust
pub fn string_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, String>
```

A string argument that must be there. Error text: ``` `<name>` is needed, as text ```.

## `host` (feature `window`)

### `struct App`

```rust
pub struct App {
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub bundle: Bundle,
    pub plugins: Vec<Arc<dyn Plugin>>,
    pub dev_url: Option<String>,
    pub debug: bool,
}
```

What the host needs to show an app. `Debug` prints title, bundle size and dev URL.

* `#[must_use] pub fn new(title: &str, bundle: Bundle) -> Self`: 1000 x 700, no plugins, no dev URL, `debug` false.
* `#[must_use] pub fn plugin(self, plugin: impl Plugin + 'static) -> Self`.
* `#[must_use] pub fn standard_plugins(self, id: &str) -> Self`: adds `DeviceState`, `SecureStorage` (app `id`), `Notifications` (the window title), `Share`, `Files` (the window's file chooser), all with the real `System` runner.
* `#[must_use] pub fn with_echo(self) -> Self`: adds `Echo`.
* `#[must_use] pub fn dev(self, url: &str) -> Self`: `dev_url = Some(url)` and `debug = true`.

### `enum RunError`

```rust
#[derive(Debug, PartialEq, Eq)]
pub enum RunError { NoDisplay }
```

`Display`: `there is no display to open a window on`. Implements `Error`.

### `run`

```rust
pub fn run(app: App) -> Result<(), RunError>
```

Shows `app` and runs until its window is closed. Steps: `Gtk::init` (also fails if GTK was already initialised on this thread); create the window; create a `WebView` with user agent suffix `QuenchHost/1`, web inspector iff `app.debug`; `view.serve("quench", ...)` with the bundle; build the `Bridge` whose `Deliver` posts, on the main loop, `window.__quenchBridge(<JSON string literal>)` to the page **only if the page's current URI is an app origin**; route the page's messages (`window.webkit.messageHandlers.quench.postMessage(text)`) to the bridge if the posting page is an app origin, else print `quench-desktop: refused origin <origin>`; navigation policy (below); load `app.dev_url` (only if `debug` is also set) or `quench://app/`; show the window; run GTK. When the window closes, GTK quits. If `QUENCH_DESKTOP_CONTROL` is set **in a debug build** (`cfg!(debug_assertions)`), the control socket listens at that path and the file is removed on exit (a failure to listen prints `quench-desktop: no control socket: <e>`).

Navigation policy: a navigation to an app origin is allowed unless it would open a new window; an `https://`, `http://` or `mailto:` link is handed to `xdg-open` (output discarded) and not followed; everything else is refused.

## `ffi` (feature `window`)

The safe types over GTK 3, webkit2gtk 4.1 and GLib. All are `!Send` and `!Sync`.

### `struct Gtk`

* `#[must_use] pub fn init() -> Option<Self>`: `gtk_init_check`; `None` without a display, and `None` if already initialised on this thread (a thread-local flag).
* `pub fn run(&self)`: runs the main loop until `quit`.
* `pub fn quit(&self)`.

### `struct Remote`

`#[derive(Debug, Clone, Copy, Default)]`. `pub fn post(self, job: impl FnOnce() + Send + 'static)`: queues `job` on the GTK main loop from any thread (`g_idle_add_full` at priority 200). If the loop has ended it never runs. A panic in the job is caught and reported on standard error.

### `struct Window`

A top-level window.

* `#[must_use] pub fn new(gtk: &Gtk, title: &str, width: i32, height: i32) -> Self`.
* `pub fn add(&self, view: &WebView)`: puts the web view in the window.
* `pub fn show(&self)`.
* `pub fn on_closed(&self, then: impl FnOnce() + 'static)`: once, when the window is destroyed.
* `pub fn close(&self)`.
* `pub fn choose(&self, folder: bool, multiple: bool, then: impl FnOnce(Option<Vec<String>>) + 'static)`: the system's file chooser over this window; `then` gets the chosen `file://` URIs or `None` if cancelled.

### `struct WebView`

A web view with a script message handler named `quench` and a URI scheme that serves assets.

* `#[must_use] pub fn new(gtk: &Gtk, application: &str, version: &str, debug: bool) -> Self`: user agent ends in `application/version`; developer extras iff `debug`.
* `pub fn serve(&self, scheme: &str, assets: impl Fn(&str) -> Option<Answer> + 'static)`: serves `scheme:` URIs (`None` is a 404). The scheme is registered secure and CORS-enabled, so the page is a secure context that can `fetch` from itself. The reason phrases used are `OK`, `Bad Request`, `Not Found`, `Method Not Allowed`, else `Status`.
* `pub fn on_message(&self, handler: impl FnMut(&str, &str) + 'static)`: each string the page posts, and the URI the page is at.
* `#[must_use] pub fn uri(&self) -> String`: current URI; empty if the window is gone.
* `pub fn on_navigate(&self, decide: impl FnMut(&str, bool) -> bool + 'static)`: asked about every navigation (link, redirect, `window.open`) with the target URI and whether it would open a new window; returning false cancels it. A new window is never opened (there is only one view): `window.open` and `target=_blank` reach `decide` with `true`.
* `pub fn load(&self, uri: &str)`.
* `pub fn eval_json(&self, script: &str, then: impl FnOnce(Result<String, String>) + 'static)`: runs `script`; `then` gets its value as JSON (`undefined` is `null`) or the error message. A promise is not awaited: its value is `{}`. After the window is gone: `Err("the window is closed")`.
* `pub fn eval(&self, script: &str)`: runs a script, result ignored.

## `control` (feature `window`)

A control socket for tests of an app in its window (debug builds only; see [internals](internals.md#control-socket)).

```rust
pub type Ask = Arc<dyn Fn(String) -> Result<String, String> + Send + Sync>;
#[must_use] pub fn answer(result: &Result<String, String>) -> String
pub fn listen(path: &Path, ask: Ask) -> std::io::Result<()>
```

* `Ask`: runs a script in the page and waits for its value as JSON.
* `answer`: the reply line: `{"ok":<json>}` or `{"error":"<message>"}`.
* `listen`: removes a stale socket at `path`, binds a Unix listener, sets mode `0600`, and serves clients on threads of their own: each line is a script, each reply one line. Empty lines are skipped.
