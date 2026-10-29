# Quench Android host

The host is a small Kotlin app that shows a [`quench-ui`](./quench-ui.md) app in a `WebView`: the same bundle that runs in a desktop browser. `quench android init` ([`quench-tool`](./quench-tool.md)) writes it into a project as `android/`; the project's `quench build --bundle` writes the bundle it serves. There is no AndroidX and no other library: a single `Activity`, a `WebViewClient` and the asset server below, about 200 lines. Native capabilities come through a plugin bridge (below); background work comes in a later stage.

```text
quench build --bundle            # dist/www: index.html and _quench/*
cd android && ./gradlew assembleDebug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

## The origin

The bundle is served from `https://app.quench.local/` by `AppAssets`, which answers `WebViewClient.shouldInterceptRequest` from the app's assets (the contents of `dist/www`, added to the main source set by `app/build.gradle.kts`). To the page this is an ordinary https origin, so cookies, storage and the content-security policy behave as they do on the web; no request leaves the device for it. (`WebViewAssetLoader` from AndroidX does the same job; writing it here keeps the host free of dependencies.)

- A path with no extension (`/app/users/7`) is a route of the app and gets `index.html`, which the router takes over. A path with an extension that is not in the bundle is a `404`. A path with a `..` segment is a `404`.
- Content types come from the extension (`html js css wasm json svg png jpg ico woff2 txt`). `_quench/` files are cached for a year (their names are hashes); everything else is `no-cache`.
- The HTML carries the policy of the server's pages: `default-src 'self'; script-src 'self' 'nonce-bundle' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`. The nonce is the fixed word `bundle` that `Ui::export` writes; it only covers the embedded state, which is not executed.
- A link to another origin opens in the system's browser (`https`, `http`, `mailto`); the WebView never navigates away from the app.

**A trap, fixed in the template:** Android's asset packaging skips directories whose name starts with `_`, which would drop `_quench/` without a word (the page then loads with every stylesheet empty and the module missing). `app/build.gradle.kts` replaces the default `ignoreAssetsPattern` for this reason.

## Dev mode (debug builds)

A debug build can show the dev server instead of the bundle, so `quench dev` reloads the app on the device:

```text
quench dev                                         # on the computer
adb reverse tcp:3000 tcp:3000                      # the device's 127.0.0.1:3000 is the computer's
adb shell am start -n <package>/.MainActivity --es dev_url http://127.0.0.1:3000
adb shell am start -n <package>/.MainActivity --es dev_url off     # back to the bundle
```

The URL is remembered (shared preferences) until switched off, so launching from the launcher keeps using it. Only the debug build allows cleartext, and only to `127.0.0.1`, `localhost` and `10.0.2.2` (the emulator's name for the computer): `app/src/debug/res/xml/network_security_config.xml`. The release build allows HTTPS only, and ignores `dev_url`. The server's own `Ui::dev_reload` script reloads the page when the server restarts, so an edit reaches the device a few seconds later (4 s measured on the emulator, the same as in a desktop browser).

## The window

- **Insets.** From Android 15 the window is edge to edge. The root view pads the WebView by the system bars and the keyboard (API 30+), so the page is never under the status bar, the gesture bar or the keyboard.
- **Back.** The manifest opts out of predictive back, so `onBackPressed` is the back button: it walks the page's history (`history.back`, which the router follows through `popstate`) and leaves the app at the start.
- **Rotation and other configuration changes** do not recreate the activity (they are listed in `configChanges`), so the page is not reloaded; the WebView state is also saved and restored if the system does recreate it.
- **Settings.** JavaScript and DOM storage on; file and content access off; mixed content never; `textZoom` 100; WebView debugging only in debug builds (`chrome://inspect`, or `scripts/webview-eval.mjs`).

## The bridge

Plugins are Kotlin classes the web app calls through typed Rust clients. The [contract](./quench-ui-web.md#the-host-bridge) (envelope, answers, error codes) and the [Rust side](./quench-ui-web-macros.md) are documented with `quench-ui-web`; this is the host's half.

- **`Plugin`** (`Plugin.kt`) is an abstract class with a `name` and `call(method, args: JSONObject, call: Call)`. **`Call`** answers once, now or later, from any thread: `resolve(value)` (a string, number, boolean, `JSONObject`, `JSONArray` or null) or `reject(code, message)`. `plugins()` in `Plugins.kt` lists the plugins the app offers; adding one is a class and a line there.
- **`Bridge`** (`Bridge.kt`) owns the port. A page gets it only when it has loaded **and its origin is the app's own**: the bundle's origin, or in a debug build the dev server's. `postWebMessage` is addressed to that origin, so no other page could receive the port anyway, and a page from anywhere else is not offered it (`quench-bridge: refused origin ...` in logcat). Navigating closes the port, so a page that is going away is not answered. Unknown plugins answer `unknown_plugin`; a `JSONException` from a plugin (a missing argument) answers `invalid_args`; any other exception answers `internal`, with the message and a log line.
- **Permissions.** `call.permissions("android.permission.X") { granted -> ... }` shows the system's dialog unless the permission is granted already, and calls back with whether all were granted. The activity forwards `onRequestPermissionsResult` to the bridge. A plugin usually rejects with `permission_denied` when it is not granted, which the app handles by code. The permission itself must be in the manifest: the debug manifest declares `POST_NOTIFICATIONS` for the demo below; a release build declares what its plugins need.
- **`EchoPlugin`** exists to try all of this and is meant to be removed: `say(text)`, `device()` (`{sdk, model}`), `add(a, b)`, `fail(code)` and `notifications()` (the permission flow). The gallery's *Native* tab calls them.

## Plugins

`quench plugin list` shows what there is and what the project has; `quench plugin add <name>` writes a plugin's Kotlin (and Rust) files, its manifest entries and its line in `plugins()`, and adds what it needs first. Adding is idempotent. The Kotlin is copied into the project, so after upgrading the quench crates run **`quench plugin update [name]`**: it writes the host code of the plugins the project has (all, or one) again from this tool's templates, and leaves the project's own Rust, the manifest and the registry alone. (A Kotlin host older than the Rust program it talks to can misread it: the surface host crashed on a tree with a field it did not know.) `build-native` also gives crates that compile C (`ring`) the NDK's compiler and archiver (`CC_<target>`, `AR_<target>`). The Rust clients are in `quench_ui_web::plugins` (typed, declared with `#[plugin]`); each method below takes and returns JSON by name, as the bridge does.

| Plugin | What it does | Needs |
|---|---|---|
| `native` | the app's Rust code on the device: `native/` is a small program, built for arm64-v8a and x86_64 and started by the host for each job | the NDK |
| `jobs` | background work: `schedule {task, interval_minutes, network, charging, battery_not_low}`, `run_now`, `cancel`, `pending`, `last_run`. A task is a native operation `job.<task>`; the system's job scheduler runs it in the app's process with no page and no activity | `native` |
| `device` | `state` (`{online, transport, metered, battery_percent, charging, battery_low, power_save}`), `watch`/`unwatch` (a `state` event on every change) | |
| `secure-storage` | `set`, `get`, `remove`, `keys`, `clear`: values encrypted with an AES-256-GCM key that stays in the Android Keystore, bound to their key name | |
| `notifications` | `channel`, `show`, `schedule {id, at_ms, title, body, channel, repeat_zone, repeat_time}`, `cancel`, `pending`, `permission`. Reminders are stored and re-armed after a reboot or a clock or zone change; a repeating one is "08:00 in Europe/Warsaw", its next time worked out by `tz.next` in `quench-android` | `native` |
| `files` | `media {kind}` (MediaStore), `pick_folder`, `pick_files` (the system picker; the grant is kept), `list_folder`, `stat`, `read {uri, offset, length}` (base64, at most 256 KiB), `upload {id, uri, url, method, headers}` (streamed from native code, events `progress`/`done`/`failed`), `cancel_upload` | |
| `packages` | `can_install`, `open_install_settings`, `installed {package}`, `list` (the apps with a screen of their own: package, label, version, `system`), `launch {package}`, `install_from_url {url}`, `install {uri}`; events `download` and `install {status, package, message}` | |
| `media` | `tts.speak/stop/voices` (events `start`, `done`, `error`), `audio.play/pause/resume/stop/seek/state` (events `ended`, `error`) | |
| `share` | `share.text/file/open`, and `intents.take` plus an `intent` event for what other apps send (a share, an opened file) | |
| `widget` | `set {title, body}`, `pin`, `count`: a home-screen widget that opens the app | |

A plugin is a Kotlin class (`Plugin`, see above), so changing one is editing a file in `android/app/src/main/kotlin/`. The manifest entries come from `quench-tool/src/plugins.rs`; a plugin removed by hand leaves its manifest lines behind, which are harmless but can be deleted.

**Native code, as a program.** `quench plugin add native` makes the crate `native/` (name `<project>-native`, a binary `quench_native`) and adds it to the workspace; `quench android build-native [--release] [--abi arm64-v8a|x86_64]` builds it with the NDK's linker (found through `ANDROID_NDK_HOME`, or the newest NDK under the SDK) and puts it in `android/app/src/main/jniLibs/<abi>/` as `libquench_native.so`: Android only extracts files with that name shape from the APK, and the host runs the result from the app's native library directory, the one place an app is allowed to run its own code from (`extractNativeLibs` and `useLegacyPackaging` are set for it). It is **not loaded into the app**: `dev.quench.Native.invoke(context, op, bytes)` starts the program, writes one request to its standard input (two length-prefixed frames: the operation name, the input) and reads one answer from its standard output (a frame: a status byte, then the result or the message), under a time limit. So there is no JNI and no `unsafe` anywhere in the native path, a crash or a hang in native code ends that process and not the app (the message it printed to standard error is in the error), and the same call works from a background job with no page. Starting it costs a few milliseconds a call, which suits jobs and reminders; it is not the place for a call per frame. The Rust side is `quench_android::register("name", handler)` for each operation and `fn main() { quench_android::run(register_ops) }`; standard output belongs to the protocol, so an operation that logs writes to standard error. `quench-android` brings `tz.next` (the next time the clock in a zone reads `HH:MM`, through daylight saving: a reading the clocks skip fires when the same time has passed, one that happens twice fires the first time).

**Background work without WorkManager.** The host uses the framework's `JobScheduler`, which is what WorkManager is built on, so there is no AndroidX dependency. Jobs are persisted (they survive a reboot; the plugin adds `RECEIVE_BOOT_COMPLETED`), the scheduler enforces the constraints, and a job started while the app is not running starts a new process that runs the task, which starts the native program. One process means the UI and a job share the same preferences and the same Keystore key: `SecureStore(context)` is the same code the plugin uses, so an auth token needs no file lock between them. (Two *processes* would; this host never has two.)

**Installing apps and Google Play.** `packages` lets the app install an APK it downloaded, which is how a store client works (hoist, for the homecloud registry). `list` needs `QUERY_ALL_PACKAGES`, which the plugin declares. Google Play's policy restricts `REQUEST_INSTALL_PACKAGES` and `QUERY_ALL_PACKAGES` to apps whose core purpose is installing or updating apps, so an app that uses the plugin cannot be distributed through Play; distribute it from your own registry. The person must allow installs from the app in the system settings (`open_install_settings`), and the system asks them to confirm every install: nothing here installs silently. An app can only update itself this way if it is signed with the same key as the new version, and the update restarts it.

## Building and running

`quench run android` installs and starts the app on the attached device; `quench dev --android` does that against the dev server; `quench build apk|aab [--release]` makes the package, signed with the key under `[android]` in `.quench.toml`; `anvil artifact release` (forge) sends the signed release to the warehouse; the tool itself never publishes. The build commands and the signing environment variables are in [`quench-tool`](./quench-tool.md#quench-run-android---abi-abi).

## Measurements

Made with `scripts/android-measure.mjs` on the `Pixel_8` emulator (x86_64, API 36, KVM, 8 cores, 2.5 GB of RAM) with a stock Flutter counter app (`flutter create`, release, `--target-platform android-x64`) as the baseline, nine cold starts each (the process stopped, then `am start -W`; TotalTime is the time to the first frame). **This is an emulator on a fast machine, not a low-end phone**, and the apps are minimal; it says how the two stacks compare under identical conditions here, not what a user of a cheap phone will see.

| | quench (signed release) | quench (debug) | Flutter counter (release) |
|---|---|---|---|
| Cold start to first frame, median | **212 ms** (201-228) | 321 ms | 621 ms (596-637) |
| Page interactive (the app's `quench:ready` mark, from the start command) | | 395 ms | |
| Fetch of the 220 KiB module | | 16 ms | |
| Memory, app process (PSS) | 77 MB | 101 MB | 51 MB |
| Memory, WebView renderer (PSS) | 74 MB | 76 MB | none |
| **Memory, total** | **151 MB** | 177 MB | **51 MB** |
| APK | **725 KiB** | 937 KiB | 16.5 MB |

What this shows: the web stack starts in about a third of the time of the Flutter counter and the package is 23 times smaller, because the engine (the WebView) is part of the system, not of the APK. It uses about three times the memory, because the renderer is a second process; that is the price of the WebView and it would not shrink with a bigger app (the page's own heap does grow). What it does not show: low-end hardware, a real app's size (daybook's release APK is 55.7 MiB; it is arm64, so it could not be measured on this x86_64 emulator, and it is a real app, not a counter), or scroll and animation smoothness on a phone, none of which could be measured here.

## Testing

`scripts/android-e2e.fish` builds the gallery bundle, the host and the APK, installs it fresh on the attached device or emulator, and checks over the WebView's DevTools port: the origin, that every stylesheet has rules, that the app took the page over, a real touch (`adb shell input tap`) opening the sheet, the back button; the bridge (`say`, a structured result, an error with its code, the permission dialog denied, then allowed, then not shown at all when granted); that the same gallery loaded from another origin (the host's refusal in logcat, and a call that fails with `unavailable`); loading the dev server through `adb reverse`; and switching dev mode off. `scripts/webview-eval.mjs` evaluates expressions, navigates, and finds where an element is on the screen to tap it; `scripts/android-dialog.mjs` presses a button of the system's UI by resource id or text.

`scripts/android-plugins-e2e.fish` is the acceptance run of the plugins: a new project with every plugin and the plugin console (`scripts/android-console/lib.rs`) as its app, built with the NDK, installed fresh, and driven with `scripts/android-plugin.mjs` while the system does what it does: the process is killed and a job still runs, the emulator reboots and a reminder still fires, the permission dialogs are pressed through the UI (the folder picker is too, but that press sequence is not yet reliable on the emulator: its three checks can fail while the picker itself works by hand), a 20 MiB file is uploaded and compared by SHA-256, and an install reaches the system's confirmation. The browser test (`scripts/ui-web-test.fish`) covers the JS half without a device: no host in a plain browser, a page that claims one, and a forged port.

## Not yet

An app icon (the system's default is used), signing and release packaging, a base URL for server functions called from the bundle (they are same-origin paths today, so a bundle app that calls `#[server]` functions needs the server's URL and a matching `connect-src`), native events pushed to the app (a plugin can only answer calls so far), `quench plugin add`, and background work.
