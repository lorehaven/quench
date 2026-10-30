---
title: Commands
parent: quench-tool
grand_parent: Crates
nav_order: 3
---

Every subcommand, taken from `src/cli.rs` and the built binary's `--help`. Commands that need a project find it from the current directory (see [the overview](index.md#global-options-and-exit-codes)). Command lines the tool runs are echoed as `$ <program> <args>` before they run.

```text
quench new | dev | build | run | linux | android | plugin | doctor | test
```

## `quench new`

```text
quench new [OPTIONS] <NAME>
```

Creates a project.

| Argument / flag | Type, default | Meaning |
|---|---|---|
| `<NAME>` | required | Lowercase letter first, then lowercase letters, digits, `-`, `_`; at most 40 characters; cannot start with `quench`. Becomes the directory and the prefix of the crate names. |
| `--template <TEMPLATE>` | `web` or `fullstack`; default `fullstack` | `web`: `app/`, `core/`, `server/`. `fullstack`: also `shared/` with a `#[server]` function the app calls. |
| `--dir <DIR>` | path, default `<cwd>/<NAME>` | Where to create it (relative paths are from the current directory). |
| `--quench-path <DIR>` | path | Depend on a checkout of the quench repository (the path is canonicalised) instead of the registry. Without it a `.cargo/config.toml` naming the `ennor` registry is also written. |

Behaviour: fails if the target exists and is not empty (`<dir> exists and is not empty`) or the name is invalid. Writes `.quench.toml` (with `name` and `template`), `Cargo.toml` (a workspace with two wasm profiles), `.gitignore` (`/target`, `/dist`, `*.keystore`, `*.jks`), `README.md`, `core/`, `app/`, `tests/browser.mjs`, `server/` and, for `fullstack`, `shared/`. Output:

```text
created demo (web) in /path/to/demo
next:  cd /path/to/demo && quench dev
```

## `quench dev`

```text
quench dev [--port <PORT>] [--open] [--android] [--linux]
```

Builds, serves, watches and rebuilds.

| Flag | Default | Meaning |
|---|---|---|
| `--port <PORT>` | `3000` | Port, passed to the server as `PORT`. |
| `--open` | off | Open `http://127.0.0.1:<port>` once, with `xdg-open` or `open` (whichever is on `PATH`; if neither exists or it fails, the URL is printed). |
| `--android` | off | Once the server listens (15 s limit), run `adb reverse tcp:<port> tcp:<port>` and start `<package>/.MainActivity` with `--es dev_url http://127.0.0.1:<port>`. A failure prints `the device did not start: <reason>` and the loop goes on. |
| `--linux` | off | Build `desktop/` and start its binary with `QUENCH_DEV_URL=http://127.0.0.1:<port>`. A failure prints `the desktop window did not start: <reason>`. Needs `quench linux init`. |

Each round: `cargo build --target wasm32-unknown-unknown --profile wasm-dev -p <name>-app`, `cargo build -p <name>-server`, then runs the server binary (`target/debug/<name>-server`) with `PORT`, `QUENCH_DEV=1` and `WASM=<module path>`. Prints `serving http://127.0.0.1:<port> - edit a file, the page reloads`. It then polls the file tree every 300 ms (see [internals](internals.md#the-file-watcher)); on a change it stops the server, prints `change detected, rebuilding` and starts over. A failed build prints `error: <reason>` and `waiting for a change`; a server that exits by itself prints `the server stopped (exit <code>); waiting for a change`. The real watcher never reports a stop, so the loop runs until the process is interrupted (the tool does not clean up the server or desktop child on SIGINT itself; only a test double can end the loop, and then the tool kills them). Build failures are not exit codes.

Example: `quench dev --port 3100 --open`.

## `quench build`

```text
quench build [--release] [--no-embed] [--bundle] [--abi <ABI>] [TARGET]
```

`TARGET` is one of `apk`, `aab`, `linux`, `deb`, `rpm`, `arch`, `source`.

| Flag | Meaning |
|---|---|
| `--release` | Plain build: size-optimised wasm (`--profile wasm`) and an optimised server with the module built in. With `apk`: a signed release. With `linux`: an optimised host. Ignored by `deb`, `rpm`, `arch` (always release) and `aab` (always release), and by `source`. |
| `--no-embed` | With plain `--release`: do not build the module in (`--features embed` omitted); the server reads it from `WASM` or `dist/app.wasm`. |
| `--bundle` | Instead: write the app as a static bundle in `dist/www`. Ignored when `TARGET` is given. |
| `--abi <ABI>` | With `apk`/`aab`: only this ABI's native library (`arm64-v8a` or `x86_64`). |

The selection order in the code: `linux`, `deb`, `rpm`, `arch`, `source`, then `apk`/`aab`, then `--bundle`, then the plain build.

### Plain build

Without `--release`: builds the module (profile `wasm-dev`) and the debug server; prints `built target/debug/<name>-server`. With `--release`: builds the module with profile `wasm` and copies it to `dist/app.wasm`; runs `wasm-opt -Oz -o dist/app.wasm dist/app.wasm` if `wasm-opt` is on `PATH` (otherwise prints `wasm-opt not found: the module is as cargo built it`); builds the server with `--release` (and `--features embed` unless `--no-embed`); copies it to `dist/<name>-server`. Output:

```text
module  dist/app.wasm (<n> KiB)
server  dist/<name>-server (<n> KiB), module built in
```

(with `--no-embed`: `, reads the module from WASM or dist/app.wasm`). Fails with `` `<command>` failed (exit <code>) `` on a failing step.

### `quench build --bundle`

Builds the module (release profile) and the debug server, deletes `dist/www` if it exists, then runs the server with `QUENCH_EXPORT=dist/www` and `WASM=<module>`; the server writes the page and the hashed assets and exits. Prints `bundle <dist>/www`. A project without `server/Cargo.toml` (a native app) prints `no server/ in this project: nothing to bundle (a native app)` and succeeds.

### `quench build apk` / `aab`

Requires an Android host (`android/`, application id from `.quench.toml` or `android/app/build.gradle.kts`). Steps: bundle; `build-native` (for the ABIs, or `--abi`) if `native/` or `surface/` exists; Gradle in `android/` with `-q <task> -Pquench.versionCode=<code> -Pquench.versionName=<name>`:

| Target | Gradle task | Built file |
|---|---|---|
| `apk`, no `--release` | `assembleDebug` | `app/build/outputs/apk/debug/app-debug.apk` |
| `apk --release` | `assembleRelease` | `app/build/outputs/apk/release/app-release.apk` |
| `aab` (release always) | `bundleRelease` | `app/build/outputs/bundle/release/app-release.aab` |

A release build needs `keystore` and `key_alias` under `[android]` in `.quench.toml`, the environment variable `QUENCH_KEYSTORE_PASSWORD` and an existing keystore file; otherwise it fails with an explanatory message. The tool sets `QUENCH_KEYSTORE` (the keystore's absolute path) and `QUENCH_KEY_ALIAS` for Gradle. The result is copied to `dist/<name>-<versionName>-<versionCode>.apk` (or `.aab`); a release APK is verified with the SDK's newest `apksigner verify --print-certs` and prints `signed: <certificate DN line>`. Last line: `<path> (<n> KiB)`. See the [packaging guide](../../guides/packaging.md).

### `quench build linux`

Needs `desktop/` (`quench linux init`). Makes the bundle, then `cargo build [-p <name>-desktop] [--release]` (in `desktop/` itself if it is a standalone crate), copies the binary to `dist/<name>-linux` and prints `linux <path>`. The binary comes from `target/<profile>/<bin>`, with `<bin>` = `[linux] bin` or `<name>-desktop`.

### `quench build deb` / `rpm` / `arch` / `source`

`deb`, `rpm`, `arch` build the release host (as `build linux --release`) and write, in `dist/`:

| Target | File | Output line |
|---|---|---|
| `deb` | `<name>_<version>_amd64.deb` | `deb <path>` |
| `rpm` | `<name>-<version>-1.x86_64.rpm` | `rpm <path>` |
| `arch` | `<name>-<version>-1-x86_64.pkg.tar.zst` | `arch <path>` |
| `source` | `<name>-<version>.tar.gz` | `source <path> (<n> files)` |

`source` builds nothing. Details in the [packaging guide](../../guides/packaging.md#linux-packages).

## `quench run`

```text
quench run [--abi <ABI>] <TARGET>      # TARGET: android | linux
```

* `run android`: needs a device: `adb devices` must list a line ending in a tab and `device` (else `no device: attach one or start an emulator (`adb devices` lists what is there)`). The ABI is `--abi` or `adb shell getprop ro.product.cpu.abi`; with native code it must be `arm64-v8a` or `x86_64`. Then: bundle, `build-native --abi <abi>` (debug) if there is native code, `./gradlew -q installDebug ...`, and `adb shell am start -n <package>/.MainActivity --es dev_url off`; prints `running <package> on the device`. The tool runs `adb` without `-s`: with several devices attached it uses adb's default selection (`ANDROID_SERIAL` if set).
* `run linux`: `build linux` (debug) and runs `dist/<name>-linux`; a non-zero exit becomes `the app stopped with exit code <n>`.

## `quench linux init`

```text
quench linux init [--id <ID>] [--quench-path <DIR>]
```

Creates `desktop/` (`Cargo.toml`, `build.rs`, `src/main.rs`).

| Flag | Default | Meaning |
|---|---|---|
| `--id <ID>` | `[linux] id`, else `com.example.<snake name>` | Application id for the keyring, notifications and menu entry. Letters, digits, `.`, `-`, `_`; at most 100 characters; not starting with `.` or `-`. |
| `--quench-path <DIR>` | the registry | Take the quench crates from a checkout. Used only when the project has no workspace `Cargo.toml` of its own (a standalone host crate). |

Fails if `desktop/` exists. If a workspace manifest exists, it adds `"desktop"` (or its path in the enclosing workspace) to `members` and `quench-desktop` to `[workspace.dependencies]`; otherwise `desktop/Cargo.toml` contains its own `[workspace]` and `/desktop/target` is appended to `.gitignore`. Appends a `[linux]` table with `id` to `.quench.toml` if there is none. Output: `created <dir>/desktop (<id>)`.

## `quench android`

### `quench android init [--package <ID>]`

Creates `android/` (20 files for a project without plugins: the Gradle project, manifest, `MainActivity`, `Bridge`, `Plugin`, `Plugins`, `EchoPlugin`, `AppAssets`, network security configs, Gradle wrapper 9.3.1). The id defaults to `com.example.<snake name>`; it must be at least two dot-separated parts of lowercase letters, digits and `_`, each starting with a lowercase letter. Fails if `android/` exists. Appends

```toml
[android]
package = "<id>"
version_name = "0.1.0"
version_code = 1
```

to `.quench.toml` unless it already contains `[android]`. Output: `created <dir>/android (<id>, <n> files)`.

### `quench android keystore [--out <FILE>] [--alias <ALIAS>]`

Makes a release key with `keytool -genkeypair -keystore <out> -alias <alias> -keyalg RSA -keysize 4096 -validity 10000 -dname CN=<name> -storepass:env QUENCH_KEYSTORE_PASSWORD [-keypass:env QUENCH_KEY_PASSWORD]`.

| Flag | Default |
|---|---|
| `--out <FILE>` | `release.keystore`, relative to the project root |
| `--alias <ALIAS>` | `release` |

Fails if the file exists (`a release key is made once and kept`), if `QUENCH_KEYSTORE_PASSWORD` is unset, or shorter than 6 characters. `QUENCH_KEY_PASSWORD` is passed only if set. Afterwards writes `keystore = "<out>"` and `key_alias = "<alias>"` to the `[android]` table (creating the table if needed) unless `keystore =` already appears in the file. Prints `made <path> and named it in .quench.toml.` and a warning to keep the key safe. **Creates a signing key; do not run it casually.**

### `quench android launcher <web|surface>`

Moves the `LAUNCHER` intent filter in `android/app/src/main/AndroidManifest.xml` between `.MainActivity` (`web`) and `.SurfaceActivity` (`surface`). Fails without `android/` (`no android/ in this project: `quench android init` makes it`), or if the target activity is missing (`this project has no surface activity: `quench plugin add surface` adds it`). Prints `the launcher starts <.Activity>`.

### `quench android build-native [--release] [--abi <ABI>]`

Builds the project's `native/` and `surface/` crates (whichever have a `Cargo.toml`; none gives `no native/ in this project: `quench plugin add native` makes it`) for each ABI (`arm64-v8a` = `aarch64-linux-android`, `x86_64` = `x86_64-linux-android`; `--abi` picks one, otherwise both) with the NDK's `<target>26-clang` as linker (API level 26). NDK: `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, else the newest directory under `$ANDROID_HOME/ndk` or `$ANDROID_SDK_ROOT/ndk`; error `the Android NDK was not found: set ANDROID_NDK_HOME (`quench doctor` says what is missing)`. It runs `cargo build -p <name>-<dir> --target <triple> [--release]` with `CARGO_TARGET_<TRIPLE>_LINKER`, `CC_<triple>` and `AR_<triple>` set. The binary (named `<name with _>_<dir>` or `quench_<dir>` in `target/<triple>/<profile>/`) is copied to `android/app/src/main/jniLibs/<abi>/libquench_native.so` or `libquench_surface.so`. Prints `<dir> <abi>: <path>` per program.

## `quench plugin`

### `quench plugin list`

Lists the eleven plugins, marking those in `android/.quench-plugins` as `added`. The list is the table in [plugins](plugins.md). Works without `android/` (nothing is marked).

### `quench plugin add <NAME>`

Adds a plugin and, first, the plugins it needs (`jobs` and `notifications` need `native`). Requires `android/` (`no android/ in this project: run `quench android init` first`). Unknown name: `no plugin `<name>`; they are: native, surface, jobs, device, secure-storage, notifications, files, packages, media, share, widget`. Already added: `<name> is already there`. Otherwise writes the Kotlin (and Rust) files that do not exist yet, inserts manifest permissions and elements at the markers, registers the plugin in `Plugins.kt`, adds crates and members to the workspace `Cargo.toml`, records the name in `android/.quench-plugins` and prints `added <name>`. For `native` and the plugins needing it it ends with `next: quench android build-native`. Idempotent.

### `quench plugin update [NAME]`

Rewrites the plugins' Kotlin and resource files under `android/app/src/main/` from this tool's templates (all added plugins, or one; a plugin the project lacks gives `this project does not have <name>: `quench plugin add <name>``). Leaves the project's own Rust, the manifest and `Plugins.kt` alone. Prints `updated <path>` per changed file, or `the plugins' host code is up to date`.

## `quench doctor`

Prints one line per check and exits 1 only if a required one is missing. Format: `  ok       <name padded to 28> <detail>`, `  absent   ...` (optional, missing) or `  MISSING  ...` (required, missing), then `everything needed for web development is here` or `fix the MISSING lines first`.

| Check | Required | What it looks for |
|---|---|---|
| `rustc` | yes | `rustc --version` >= 1.85 |
| `cargo` | yes | `cargo --version` |
| `wasm32 target` | yes | `<sysroot>/lib/rustlib/wasm32-unknown-unknown` |
| `wasm-opt` | no | on `PATH` |
| `node 22+ (browser tests)` | no | `node --version` (any version is accepted; the label says 22+) |
| `chromium (browser tests)` | no | `chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable` |
| `android sdk` | no | `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `~/Android/Sdk`, `/opt/android-sdk` (the first that exists) |
| `android ndk` | no | `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, else the newest in the SDK's `ndk/` |
| `adb` | no | on `PATH` |
| `android rust targets` | no | `aarch64-linux-android`, `x86_64-linux-android` |
| `java` | no | `java -version` major >= 17 |
| `gradle` | no | on `PATH` (a project's wrapper is used regardless) |
| `webkit2gtk (linux desktop)` | no | `pkg-config --modversion webkit2gtk-4.1` or `webkitgtk-6.0` |
| `desktop helpers` | no | `secret-tool`, `notify-send`, `xdg-open` on `PATH` |

Sample output (this machine):

```text
quench doctor
  ok       rustc                        rustc 1.98.1 (48a229cea 2026-09-01)
  ok       cargo                        cargo 1.98.1 (797e8a9bc 2026-08-05)
  ok       wasm32 target                installed
  absent   wasm-opt                     optional, shrinks release modules: install binaryen
  ...
everything needed for web development is here
```

## `quench test`

```text
quench test [--browser] [ARGS]...
```

Runs `cargo test --workspace <ARGS>` (a project inside a larger workspace: `cargo test -p <each crate directly below the project root> <ARGS>`), then `cargo check --target wasm32-unknown-unknown -p <name>-app`. With `--browser`: needs `tests/browser.mjs`, `node` and a Chromium on `PATH`; builds the module and server (`wasm-dev`, debug), starts the server on a free port with `PORT` and `WASM` (without `QUENCH_DEV`), waits up to 15 s for it, runs `node tests/browser.mjs http://127.0.0.1:<port>` with `CHROMIUM=<browser>`, then kills the server.

## Errors common to all commands

`error: <message>` on standard error and exit 1. Examples: `` `cargo build ...` failed (exit 101) ``, `cannot run `<program>`: <io error>`, `.quench.toml: unknown template `x` (web or fullstack)`, `.quench.toml: <parse error>`.
