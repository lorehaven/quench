---
title: The Android host app
parent: quench-android
grand_parent: Crates
nav_order: 3
---

The Kotlin app that `quench android init` writes into a project as `android/`. It has no crate of its own (the files are templates in `quench-tool/templates/android/`, see [quench-tool internals](../quench-tool/internals.md#android-from-quench-android-init)); the Rust crate [quench-android](index.md) is only the native-program side. This page describes the host from those templates.

A single `Activity` (`MainActivity`) shows one `WebView`; there is no AndroidX and no other library. Build and run: `quench build apk`, `quench run android`, `quench dev --android` ([commands](../quench-tool/commands.md)).

## The origin

`AppAssets` answers `WebViewClient.shouldInterceptRequest` from the app's assets (the contents of `dist/www`, added with `assets.srcDir(file("../../dist/www"))`). The bundle is served from `https://app.quench.local/` (`AppAssets.HOST`, `ORIGIN`), so cookies, storage and the content-security policy behave as on the web; no request leaves the device for it.

* A path with no `.` in its last segment (and `/`) is a route of the app and gets `index.html`. A path with a file extension that is not in the bundle is `404` (`not found`, `Cache-Control: no-store`). A path with a `..` segment is `404`.
* Content types by extension (case-insensitive): `html js mjs css wasm json svg png jpg jpeg ico woff2 txt`; otherwise `application/octet-stream`. `.html` responses declare `utf-8`.
* Headers: `Cache-Control: public, max-age=31536000, immutable` for paths under `/_quench/` (their names are hashes), `no-cache` for everything else; `X-Content-Type-Options: nosniff`. HTML also gets `Referrer-Policy: same-origin` and
  `Content-Security-Policy: default-src 'self'; script-src 'self' 'nonce-bundle' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`. The nonce is the fixed word `bundle` that `Ui::export` writes.
* A link to another origin opens in the system's browser (`ACTION_VIEW`) for `https`, `http` and `mailto`; every other navigation away from the app is refused. The `WebView` never leaves the app.
* **Trap handled in the template:** Android's asset packaging skips directories whose name starts with `_`, which would drop `_quench/`. `app/build.gradle.kts` replaces the default `ignoreAssetsPatterns` (`!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~`).

## Dev mode (debug builds)

A debug build can show the dev server instead of the bundle:

```bash
quench dev                                         # on the computer
adb reverse tcp:3000 tcp:3000                      # the device's 127.0.0.1:3000 is the computer's
adb shell am start -n <package>/.MainActivity --es dev_url http://127.0.0.1:3000
adb shell am start -n <package>/.MainActivity --es dev_url off     # back to the bundle
```

`quench dev --android` does the first two for you. The URL is kept in the preferences `quench-dev` (key `dev_url`) until `off` clears it, so launching from the launcher keeps using it. Only a debug build honours it: a release build always loads `https://app.quench.local/`. Cleartext HTTP is allowed only in the debug build, and only to `127.0.0.1`, `localhost` and `10.0.2.2` (`app/src/debug/res/xml/network_security_config.xml`); the release configuration has `cleartextTrafficPermitted="false"` only.

## The window

* **Insets.** The root `FrameLayout` pads the `WebView` by the system bars and the keyboard (`systemBars() | ime()`, API 30 and later), so the page is never under the status bar, the gesture bar or the keyboard.
* **Back.** The manifest sets `android:enableOnBackInvokedCallback="false"`, so `onBackPressed` is used: it walks the page's history (`web.goBack()`) and leaves the app at the start.
* **Configuration changes** (`orientation|screenSize|smallestScreenSize|screenLayout|keyboard|keyboardHidden|uiMode|density|fontScale|layoutDirection|locale`) do not recreate the activity. If the system does recreate it, the `WebView` state is saved and restored.
* **Settings.** JavaScript and DOM storage on; file access and content access off; mixed content never allowed; `textZoom` 100; user agent suffixed with `QuenchHost/1`; remote debugging (`chrome://inspect`) only in debug builds.
* **Launch mode** `singleTop`; `windowSoftInputMode` `adjustResize`; `minSdk` 26, `targetSdk` and `compileSdk` 36.
* **Intents.** An intent from another app (action not `MAIN`) is offered to the plugins (`Plugin.onIntent`), at start and in `onNewIntent`.

## The bridge

See [the bridge contract](../quench-tool/plugins.md#the-bridge-contract-host-side). The page gets the `MessagePort` in `onPageFinished` if its origin is the app's (or, in a debug build, the dev server's); `onPageStarted` closes it. A refused origin logs `refused origin <origin>: the page gets no bridge` (tag `quench-bridge`).

## Signing and versions

See the [packaging guide](../../guides/packaging.md#android).
