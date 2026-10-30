---
title: Desktop and Android hosts
parent: Guides
nav_order: 14
---

A Quench UI app can run in four places: a browser, a Linux desktop window, an Android `WebView` app, or (without a web page) drawn by the native renderer in a Wayland window or on an Android `SurfaceView`. This guide shows how the hosts relate and how a page reaches native features. Reference pages: [quench-desktop](../crates/quench-desktop/index.md), [quench-android](../crates/quench-android/index.md), [quench-android-surface](../crates/quench-android-surface/index.md), [quench-desktop-native](../crates/quench-desktop-native/index.md), [quench-desktop-wayland](../crates/quench-desktop-wayland/index.md), [quench-desktop-atspi](../crates/quench-desktop-atspi/index.md), [quench-tool](../crates/quench-tool/index.md).

## The two kinds of host

| | Web bundle host | Native-renderer host |
|---|---|---|
| Linux | `desktop/` crate with `quench-desktop` (GTK + webkit2gtk) | `quench-desktop-native` (Wayland window; `quench-desktop-wayland` + `quench-desktop-atspi`) |
| Android | `android/` Gradle project (Kotlin, `WebView`) | the `surface` plugin: `SurfaceActivity` + a Rust program using `quench-android-surface` |
| Creates | `quench linux init`, `quench android init` | `quench plugin add surface`, `quench android launcher surface`; the desktop one is a library, run it from your own `main` |
| Content | the bundle `dist/www` (`quench build --bundle`) | the program draws views itself (no page) |

Both kinds call native features through **plugins** with the same envelope.

## The plugin bridge

Request: `{"id": 1, "plugin": "device", "method": "state", "args": {}}`. Answer: `{"id": 1, "ok": <value>}` or `{"id": 1, "err": {"code": "...", "message": "..."}}`. Event: `{"event": "state", "plugin": "device", "data": {...}}`.

| Host | Page to host | Host to page | Origin served |
|---|---|---|---|
| Android | a `MessagePort` handed to the page after load, only if the origin is the app's | the same port | `https://app.quench.local/` (debug: also the dev server) |
| Desktop | `window.webkit.messageHandlers.quench.postMessage(text)` | the host evaluates `window.__quenchBridge("<json>")` | `quench://app/` (debug: also the dev server) |
| Native-renderer program | `quench_android_surface::Bridge::call` writes the envelope on its standard output | `Bridge::on` handlers and `call` closures | none |

Plugin names and methods are the same on both web hosts where the desktop can do the thing. The Android host registers `echo` (to try the bridge) and whatever `quench plugin add` added; the desktop `standard_plugins` registers `device`, `secure_storage`, `notifications`, `share`, `files`.

| Plugin | Android | Desktop |
|---|---|---|
| `echo` | `EchoPlugin` (always) | `App::with_echo()` |
| `device` | `quench plugin add device` | `state`, `watch`, `unwatch` from `/sys` (polls every 5 s) |
| `secure_storage` | Keystore, AES-256-GCM (`secure-storage` plugin) | `secret-tool` (libsecret) |
| `notifications` | alarms, reboot-safe, repeating by zone | `notify-send`; reminders only while the app runs; repeating: `unsupported` |
| `share` | chooser, `intents` for incoming | `xdg-open` (`mailto:` for text) |
| `files` | MediaStore, system picker, uploads | picker; only what was picked in this run; `media`/`upload`/`cancel_upload`: `unsupported` |
| `jobs`, `packages`, `tts`, `audio`, `widget`, `intents` | Android only | not offered (a call answers `unknown_plugin`) |

Differences to watch: `files.pick_files` defaults `multiple` to **true** on Android and **false** on the desktop; `notifications.schedule` on the desktop does not survive the app; `secure_storage.keys` on the desktop depends on `secret-tool search` output.

### Writing a desktop plugin

See [quench-desktop usage](../crates/quench-desktop/usage.md#3-a-plugin-of-your-own). On Android a plugin is a Kotlin `Plugin` subclass listed in `plugins()` ([details](../crates/quench-tool/plugins.md#the-bridge-contract-host-side)).

## Developing against a host

```bash
quench dev --linux      # desktop window on the dev server; needs `quench linux init`
quench dev --android    # device or emulator loads the dev server through adb reverse
```

The desktop host receives `QUENCH_DEV_URL`; the Android host receives the intent extra `dev_url` (remembered until `--es dev_url off`). Debug builds only: the release host ignores both. Cleartext HTTP is allowed only for `127.0.0.1`, `localhost` and `10.0.2.2` on Android.

## Native code on Android

`quench plugin add native` creates `native/`, a small Rust program the host runs as a **separate process** (no JNI): one request on its standard input, one answer on its standard output ([protocol](../crates/quench-android/internals.md)). `quench android build-native` builds it with the NDK for `arm64-v8a` and `x86_64` and installs it as `libquench_native.so`. Background `jobs` and repeating `notifications` use it (`job.<task>`, `tz.next`).

## Native renderer on Linux

`quench_desktop_native::run(title, app_id, size, &renderer)` opens a Wayland window ([`quench-desktop-wayland`](../crates/quench-desktop-wayland/index.md)), draws when the renderer is dirty, handles pointer, wheel, keyboard, key repeat, the input method (text-input v3) and the clipboard, and publishes an AT-SPI accessibility tree for screen readers such as Orca ([`quench-desktop-atspi`](../crates/quench-desktop-atspi/index.md)). There is no `quench` subcommand for it; you write a `main` as in the [overview](../crates/quench-desktop-native/index.md#minimal-example).

## Accessibility

* Web hosts: the web view's own accessibility.
* Native renderer on Linux: AT-SPI objects (`Application`, `Window`, nodes) with the `Accessible`, `Component`, `Action`, `Text`, `EditableText` and `Value` interfaces, and signals for name, text, caret, checked/expanded/selected, children and focus changes ([coverage](../crates/quench-desktop-atspi/atspi.md)).
* Native renderer on Android: the program sends its tree to the host, which exposes it through an `AccessibilityNodeProvider` ([protocol](../crates/quench-android-surface/internals.md#the-accessibility-tree)).

## Packaging

See [Packaging apps](packaging.md).

## Not covered here

Running any of the windows: the authors of this book did not start desktop windows or touch devices while writing it.
