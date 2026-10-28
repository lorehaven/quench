# Quench desktop host (Linux)

`quench-desktop` (crate `quench_desktop`) shows a [`quench-ui`](./quench-ui.md) app in a native window: a GTK 3 window around webkit2gtk 4.1, serving the app's bundle from inside the binary and offering the same plugin bridge as the [Android host](./quench-android.md). It is the same bundle that runs in a browser and in the Android `WebView`; nothing in the UI code changes. [`quench-tool`](./quench-tool.md) makes the host crate (`quench linux init`), builds it (`quench build linux`), packages it (`quench build deb`) and runs it (`quench run linux`, `quench dev --linux`).

```text
quench linux init                # desktop/ : the host crate, joined to the workspace
quench run linux                 # bundle, build, open the window
quench build linux --release     # dist/<name>-linux, one binary, runs offline
quench build deb                 # dist/<name>_<version>_amd64.deb
```

A machine needs `webkit2gtk-4.1` and `gtk3` (the libraries are linked by name, no headers or C compiler are involved); `quench doctor` checks for them and for the helper programs the plugins use.

**The `window` feature.** Everything that needs GTK to link (the FFI, the window, the control socket) is behind the crate's `window` feature, which is **off by default**. Without it the crate is the bundle and its routing, the bridge, the plugins and the origin rules: it builds, tests and packages on a machine with no GTK (a CI image, `cargo package`), and nothing it produces links `gtk-3` or `webkit2gtk`. The generated `desktop/` crate depends on it with `features = ["window"]`, and so does anything else that wants a window. To work on the window itself: `cargo test -p quench-desktop --features window` (the window tests need a display and skip without one); `scripts/coverage.fish` uses `--all-features`, so it needs the libraries too. A project made with `quench linux init` therefore needs them for `cargo build --workspace` and `quench test`.

## How the window is built

The host is `quench_desktop::run(App)`. An `App` is a title, a `Bundle` (the files of `dist/www`), plugins and, in a debug build, a dev-server URL. The generated `desktop/` crate embeds the bundle with its `build.rs` (one `include_bytes!` per file under `dist/www`, written into `OUT_DIR`), so the binary needs nothing next to it.

- **The origin.** The bundle is served by a custom URI scheme: `quench://app/`. It is registered secure and CORS-enabled, so the page is a secure context that can `fetch` and instantiate its module from its own origin, and cookies, storage and the content-security policy behave as on the web. The rules are the Android host's: a path with no extension is a route of the app and gets `index.html`; a path with an extension that is not in the bundle is a `404` (a real status, through `WebKitURISchemeResponse`); a path with `..`, `.`, a backslash or an encoded separator is a `404`; another host or scheme is a `404`. Content types come from the extension.
- **Navigation.** The view never leaves the app's origin. A link or script navigation to `https:`, `http:` or `mailto:` is handed to `xdg-open` and the page stays where it is; anything else (`file:`, `javascript:`, another scheme) goes nowhere. `window.open` and `target=_blank` are never opened as windows (there is one view): the same rule applies to their address.
- **Detection.** The user agent ends in `QuenchHost/1`, as on Android, so a page knows a host exists.
- **Dev mode (debug builds only).** `App::dev(url)` (and the generated `main.rs`, from `QUENCH_DEV_URL`) shows a dev server instead of the bundle, allows its origin for the bridge, and turns on the web inspector (right-click, *Inspect Element*). `quench dev --linux` starts the host once the server listens and leaves the window open across server restarts: the server's own reload script refreshes the page. A release build ignores all of it.

## The bridge

The contract is the one in [`quench-ui-web`](./quench-ui-web.md#the-host-bridge): calls are fetches of `quench-bridge:call`, events an event source on `quench-bridge:events`, the envelopes are `{"id","plugin","method","args"}` answered by `{"id","ok"}` or `{"id","err":{"code","message"}}`, and the typed clients in `quench_ui_web::plugins` work unchanged. Only the transport differs, because webkit2gtk cannot give a page a `MessagePort`:

- **Page to host** is `window.webkit.messageHandlers.quench.postMessage(text)`, which `quench.js` uses when the host is detected and the handler exists. The host reads the message **together with the page's URI** and answers it only if the page is the app's own (`quench://app`, or the dev server in a debug build); any other origin is refused (`quench-desktop: refused origin ...` on standard error), and messages over 1 MiB are dropped.
- **Host to page** is a script the host runs in the page, `window.__quenchBridge(<the JSON text, quoted>)`, which `quench.js` defines (read-only) when it starts. The host runs it only while the page is the app's own, so a page that is going away is not answered.
- **A limit.** WebKit's message handler does not say which frame posted. A script in the app's own page can post (that is what the page is for), but so could a frame inside it; the content-security policy of the bundle (`default-src 'self'`, `frame-ancestors 'none'`) is what keeps other origins out of the page, where Android's port is given to the top frame alone. Do not add a framing origin to the policy of an app that uses plugins.

In Rust a plugin is a type implementing `Plugin` (`name`, `call(method, &args, Call)`, and `attach(Emitter)` to push events). A `Call` answers once, now or later, from any thread: `resolve(&value)` or `reject(code, message)`, and it consumes itself, so it cannot answer twice. The bridge answers an unknown plugin with `unknown_plugin`, and a plugin that panics with `internal` (the window goes on).

## Plugins

`App::standard_plugins(id)` adds the ones every app can use (`id` is the application id, `dev.lorehaven.notes`); `with_echo()` adds `echo`, which the gallery's *Native* tab calls. The names and methods are the Android ones where a desktop has the same thing, so one typed client serves both; what a desktop cannot do answers `unsupported`.

| Plugin | On the desktop | Through |
|---|---|---|
| `echo` | `say`, `device` (`{sdk: 0, model: <hostname>}`), `add`, `fail`, `notifications` (always `true`) | |
| `device` | `state` (`{online, transport, metered, battery_percent, charging, battery_low, power_save}`), `watch`/`unwatch` (a `state` event on every change, looked for every 5 s). Network from `/sys/class/net` (hardware interfaces that are up: `wifi`, `ethernet`, `cellular`, `other`), battery and mains from `/sys/class/power_supply` (`-1` without a battery), `power_save` from the ACPI platform profile. `metered` is `false` | `/sys` |
| `secure_storage` | `set`, `get`, `remove`, `keys`, `clear`: items in the desktop's keyring with the attributes `app` (the application id) and `key` | `secret-tool` (libsecret: GNOME Keyring, KWallet's Secret Service, KeePassXC). The value goes on standard input, never on a command line. `keys` reads the attributes `secret-tool search` prints (libsecret writes them on its **error** output, and the secrets on standard output, which is not kept). Needs a running secret service: without one the call answers `unavailable` |
| `notifications` | `show` posts through `notify-send` (the same id replaces the notification); `schedule`/`cancel`/`pending` keep reminders **in this process** (one scheduler thread, however many there are) and post them when due; `permission` is always granted; `channel` does nothing. A reminder does not outlive the app, and `repeat_zone`/`repeat_time` answer `unsupported` (a desktop has nothing that wakes a closed app: schedule the next one when one fires) | `notify-send` |
| `share` | `open` and `file` hand a `file:`, `http(s):` or `mailto:` address to `xdg-open`; `text` opens a `mailto:` compose. Anything else answers `invalid_args`, so nothing that looks like an option reaches the program | `xdg-open` |
| `files` | `pick_folder`, `pick_files {multiple}` (the system's file chooser, a native dialog: the portal on a sandboxed or Wayland desktop), then `list_folder`, `stat` and `read {uri, offset, length}` (base64, at most 256 KiB) on what was picked. Cancelling answers `cancelled`. `media`, `upload` and `cancel_upload` answer `unsupported` | GTK's `GtkFileChooserNative` |

**`files` only reads what the user picked.** A page can name any path in a `file://` URI, so the plugin grants access only to what the user handed over through the chooser **in this run**: a picked file, or a picked folder and what is under it (paths are canonicalised, so a symbolic link that leaves the folder is neither listed nor readable). Anything else answers `permission_denied`, as an Android folder that was never granted does. Grants do not outlive the run. The check is made on **the descriptor that was opened** (its path is read back from `/proc/self/fd`), not on a path that is looked up again, so a symbolic link swapped in between the check and the read cannot redirect the read; a folder is listed through the descriptor too. Without `/proc` nothing can be opened (it fails closed). Files are opened without blocking, so a named pipe in a picked folder cannot hang a call (`stat` and `list_folder` answer, a read returns what is there, and a read at an offset past the start of a pipe is an error since a pipe cannot seek).

The programs are run through a `Runner` (the real one is `System`), so the plugins' tests see exactly what would have been run and answer from a script; a missing program is `unavailable` with its name.

## The `unsafe`, and why it is in one file

The crate calls GTK, webkit2gtk and GLib, which are C libraries; there is no safe way to do that, and a process boundary would only move the same calls. Following the rule that `unsafe` without an alternative is allowed when it is confined, tested and described, **every `unsafe` of the crate is in `src/ffi.rs`** and the file is on the security gate's allowlist with this reason. The rest of the crate is safe code over the types it exports (`Gtk`, `Window`, `WebView`, `Remote`).

The contract those types keep (it is also the first thing in `ffi.rs`):

- **One thread.** GTK may only be used from the thread that initialised it. `Gtk::init` hands out one `Gtk`, and `Gtk`, `Window` and `WebView` are `!Send` and `!Sync` (a `PhantomData<*const ()>`), so they cannot reach another thread. `Remote::post(job)` is the one way in from another thread: it queues the job on the main loop with `g_idle_add_full`, which GLib allows from any thread. Plugins that finish on threads of their own answer through it.
- **Ownership.** A widget belongs to GTK. A `WebView` lives in its `Window`; when the window is destroyed its `alive` flags are cleared, and the safe methods then do nothing instead of using a dead pointer.
- **Callbacks.** A Rust closure given to GTK is boxed, its pointer is the callback's user data, and GTK frees it through a destroy notify when the signal handler or the scheme goes. One that GIO calls exactly once (a script's result) takes its box back in the callback. A closure never unwinds into C: it runs under `catch_unwind` and a panic is reported on standard error.
- **Strings.** Text for C is a `CString` that lives for the call; text from C is copied into a `String` before the callback returns, and freed with `g_free` where the C side hands over ownership.

**How it is tested.** The pure parts (the bundle's routing and hostile paths, the bridge, every plugin against a fake runner and a temporary `/sys`, origins, the navigation rules) are plain unit tests. The calls into GTK are tested through real windows: `tests/fixture` is a binary that opens one, and the tests run it and drive it. They need a display and skip, saying so, when there is none. The window tests cover the custom scheme (routes, a `404`, a secure context, the user agent), a message round trip, the bridge with the echo and standard plugins, a script that throws, and links leaving the app (a stand-in `xdg-open` records what it is given). With `QUENCH_DESKTOP_REAL=1` one more test uses the **real** keyring and notification service (it stores, reads, lists, removes and clears items under a throwaway application id, and posts one notification, which appears on the screen); it found that libsecret prints attributes on stderr, which the stand-in had hidden. What is not tested automatically is the file chooser's dialog, which needs a person: its code is the one uncovered block of `ffi.rs`, and the plugin around it is tested with a stand-in chooser.

## Driving a window from a test (debug builds)

With `QUENCH_DESKTOP_CONTROL=<path>` a **debug** build listens on a Unix socket (mode `0600`): each line a client sends is a script to run in the page, and the answer is one line, `{"ok":<the value as JSON>}` or `{"error":"..."}`. A release build ignores the variable. `scripts/linux-eval.mjs <socket> "<script>"` is the client, and `scripts/linux-e2e.fish` uses it the way the Android acceptance uses `scripts/webview-eval.mjs`. A promise is not awaited (its value is `{}`): a script that starts asynchronous work returns something plain and the test polls for the result.

## Packaging

`quench build deb` makes `dist/<name>_<version>_amd64.deb` from the release host: the binary in `/usr/bin/<name>` and a menu entry, `/usr/share/applications/<id>.desktop`. The archive is written by the tool itself (an `ar` file of `debian-binary`, `control.tar.gz` and `data.tar.gz`, with `quench-archive`), and `dpkg-deb -I` and `-c` read it. It depends on `libwebkit2gtk-4.1-0` and `libgtk-3-0`. The version is the workspace's. **AppImage is not made:** it needs `appimagetool` and a bundled webkit, which is a decision of its own.

## Not done

- Windows (WebView2) and macOS (WKWebView) hosts: only on request, as the plan says. The crate is `#![cfg(target_os = "linux")]`.
- `jobs`, `media`, `tts`/`audio`, `packages`, `intents` and `widget` have no desktop plugin yet; the typed clients answer `unavailable` (`unknown_plugin`) for them.
- Grants for `files` do not persist, and uploads are not offered.
- The desktop transport has been tested on one machine (Arch, webkit2gtk 2.54, Hyprland on Wayland). X11 and other webkit versions are not checked.
