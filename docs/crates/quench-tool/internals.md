---
title: Internals
parent: quench-tool
grand_parent: Crates
nav_order: 6
---

## Generated projects

`quench new demo --template fullstack` writes:

```text
.quench.toml  Cargo.toml  .gitignore  README.md  .cargo/config.toml (registry builds)
core/    Cargo.toml src/lib.rs tests/unit.rs        logic with no interface, tested natively
app/     Cargo.toml src/lib.rs                      cdylib (the module) and rlib (server-side render); ends with quench_ui_web::app!(app)
server/  Cargo.toml src/main.rs                     a quench-http server using quench-fullstack; feature `embed` includes dist/app.wasm
shared/  Cargo.toml src/lib.rs                      fullstack only: types and #[server] functions
tests/browser.mjs                                   the smoke test for `quench test --browser`
```

The root `Cargo.toml` is a workspace with `[workspace.package] version = "0.1.0", edition = "2024"`, the pinned quench dependencies, `tokio`, and two profiles: `wasm` (inherits `release`; `opt-level = "z"`, `lto`, `codegen-units = 1`, `panic = "abort"`, `strip`) and `wasm-dev` (inherits `dev`). The server template reads `PORT` (default 3000), `WASM`, `QUENCH_DEV` (calls `Ui::dev_reload()`), and `QUENCH_EXPORT` (writes the bundle with `Ui::export` and exits); it binds `127.0.0.1`.

## The bundle

`dist/www/`: `index.html` (the page rendered on the server) and the hashed assets under `_quench/`. Hosts serve it from their own origin: `https://app.quench.local/` (Android, see [quench-android](../quench-android/index.md)) and `quench://app/` (desktop, see [quench-desktop](../quench-desktop/internals.md)).

## `android/` (from `quench android init`)

Gradle project (Gradle 9.3.1 wrapper, `compileSdk 36`, `targetSdk 36`, `minSdk 26`, JVM 17, Kotlin, no AndroidX). `versionCode` and `versionName` come from the Gradle properties `quench.versionCode` / `quench.versionName` (defaults 1 and `0.1.0`). Assets include `../../dist/www`; `androidResources.ignoreAssetsPatterns` is replaced so `_quench/` is packaged (aapt skips `_`-prefixed directories by default). `useLegacyPackaging` and `extractNativeLibs` are set so `libquench_native.so` can be run from the native library directory. The release signing config reads `QUENCH_KEYSTORE`, `QUENCH_KEYSTORE_PASSWORD`, `QUENCH_KEY_ALIAS`, `QUENCH_KEY_PASSWORD` from the environment; the release build type uses it only when `QUENCH_KEYSTORE` is set (an unsigned release APK is produced otherwise, though `quench build apk --release` refuses to run without a key). Minification is off. Release network security: no cleartext; debug: cleartext only to `127.0.0.1`, `localhost`, `10.0.2.2`. `MainActivity` takes the intent extra `dev_url` (debug builds): a URL to show instead of the bundle, remembered in the preferences `quench-dev` until a start with `--es dev_url off`.

## `desktop/` (from `quench linux init`)

`Cargo.toml` (package `<name>-desktop`, `[[bin]] name = "<name>-desktop"`, dependency `quench-desktop` with feature `window`), `build.rs` (writes `bundle.rs` into `OUT_DIR` with one `include_bytes!` per file of `dist/www`; warns `no bundle in dist/www: run `quench build --bundle` first` when empty) and `src/main.rs`:

```rust
let mut app = quench_desktop::App::new("<title>", bundle()).standard_plugins("<id>");
if cfg!(debug_assertions) {
    app = app.with_echo();
    if let Ok(url) = std::env::var("QUENCH_DEV_URL") { app = app.dev(&url); }
}
quench_desktop::run(app)
```

A failure prints `<name>: <error>` and exits with `ExitCode::FAILURE`.

## The file watcher

`watch::fingerprint` hashes `(relative path, mtime, size)` of every file whose extension is `rs`, `toml`, `ftl`, `css`, `js` or `html`, skipping hidden directories and `target`, `dist`, `node_modules`. `Poller::poll` sleeps one interval (300 ms), compares, and on a change waits in half-intervals until the fingerprint stops changing (an editor saving in steps).

## `System`

All side effects go through `system::System`, so commands are tested with a recording fake (`tests/unit/support.rs`) and no cargo, gradle or adb runs in the tests.

## Package builders

* `.deb`: an `ar` archive (`debian-binary` = `2.0\n`, `control.tar.gz`, `data.tar.gz`) written with `quench-archive`. `ar` members have mtime/uid/gid 0 and mode `100644`.
* `.rpm`: lead, signature header (`SIZE`, `PAYLOADSIZE`, SHA-256 of the header, region tag 62), header (region tag 63) and a gzip-compressed `cpio` (newc) payload; file digests SHA-256; no dependencies and no signature. Release `1`, arch `x86_64`, license `unspecified`, build host `localhost`.
* Arch: a zstd-compressed tar with `.PKGINFO` (pkgname, pkgbase, `pkgver = <version>-1`, pkgdesc, builddate, packager `Unknown Packager`, size, `arch = x86_64`) and the files; no `.MTREE`, no dependencies, no signature.
* Source: tar.gz `<name>-<version>/…`, skipping directories `target`, `dist`, `.git`, `.idea`, `.gradle`, `build`, `node_modules`, files `local.properties`, `.DS_Store`, and extensions `keystore`, `jks`, `so`.

## Limits and notes

* All Linux packages are `amd64`/`x86_64` only.
* `quench dev` serves HTTP on `127.0.0.1` only.
* `build-native` links for API level 26 (`minSdk`).
* `System::http` exists but no command uses it.
