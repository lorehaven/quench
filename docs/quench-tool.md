# Quench Tool

`quench-tool` provides the `quench` command: one command per step of building a [`quench-ui`](./quench-ui.md) app, in the way of the `flutter` CLI. It builds on [`quench-args`](./quench-args.md) (the command line), [`quench-template`](./quench-template.md) (the project files) and nothing else; it runs `cargo` for the building, so there is no build system of its own.

```text
quench new demo            # a project: app/, core/, server/, shared/
cd demo
quench dev                 # build, serve on http://127.0.0.1:3000, reload the browser on every change
quench test                # the tests, and a check that the app builds for the browser
quench build --release     # dist/: the module and one server binary with the module inside
quench doctor              # what this machine has and lacks
```

Install it with `cargo install quench-tool --registry ennor`; the binary is `quench`.

## `quench new <name>`

| Option | |
|---|---|
| `--template web` | `app/`, `core/`, `server/`: a counter, rendered on the server and taken over by the browser |
| `--template fullstack` (default) | the same plus `shared/`: a `#[server]` function (`greet`) the app calls |
| `--dir <DIR>` | where to create it (default `./<name>`; it must be empty or not exist) |
| `--quench-path <DIR>` | depend on a checkout of the quench repository instead of the registry; how the tool's own tests and the roadmap's acceptance runs work before the crates they need are released |

The name is a lowercase letter followed by letters, digits, `-` and `_`, at most 40 characters, and cannot start with `quench`. It becomes the directory and the prefix of the crates (`demo-app`, `demo-core`, `demo-server`, `demo-shared`).

| Crate | What it is |
|---|---|
| `tests/browser.mjs` | the browser smoke test `quench test --browser` runs |
| `core/` | logic with no interface (`step`, here), tested natively with `quench-test` |
| `shared/` | fullstack only: the types that cross the wire and the `#[server]` functions; built for the server and the browser |
| `app/` | the interface: `cdylib` (the module the browser loads) and `rlib` (what the server renders pages from); ends with `quench_ui_web::app!(app)` |
| `android/` | after `quench android init`: the Android host |
| `server/` | a `quench-http` server using `quench-fullstack`: the page, the hashed assets, the server functions. Feature `embed` builds the module in |

The root `quench.toml` (`name`, `template`) is how the other commands recognise a project; they find it from any directory below. The workspace defines two wasm profiles: `wasm-dev` (quick to compile, used by `dev` and a plain `build`) and `wasm` (size-optimised: `opt-level = "z"`, LTO, `panic = "abort"`, stripped). Without `--quench-path` the project depends on the registry `ennor` and gets a `.cargo/config.toml` naming it.

## `quench dev [--port N] [--open]`

Builds the module (`cargo build --target wasm32-unknown-unknown --profile wasm-dev`) and the server, starts the server with `PORT`, `WASM` (the module's path) and `QUENCH_DEV=1`, then watches the project. When a source file changes it stops the server, rebuilds and starts it again.

- **Watching** is by modification time every 300 ms, over `.rs`, `.toml`, `.ftl`, `.css`, `.js` and `.html` files, skipping `target/`, `dist/`, `node_modules/` and hidden directories. A burst of writes (an editor saving) is waited out before the rebuild.
- **Reloading the browser.** With `QUENCH_DEV` set, the generated server calls `Ui::dev_reload()` ([`quench-fullstack`](./quench-fullstack.md#development-reload)): every page carries `/_quench/dev.js`, which holds the server-sent events stream `/_quench/dev` open. When the stream drops (the server is restarting) the script polls until the server answers and reloads the page. It is a full reload; keeping the app's state across a rebuild is not attempted.
- **A failed build** is printed and waited out: the old server is already stopped, and the next change tries again. A server that exits by itself is reported once.
- `--open` opens the page in the default browser once the first build is serving (`xdg-open`, or `open` on macOS; if neither exists or it fails, the URL is printed). It opens once: restarts reload the page that is already open.
- The server's in-memory state is lost on every restart; that is what a development server looks like.

Measured on this machine (a generated fullstack project, warm cache): an edit to `app/src/lib.rs` is served by the new server 4 s later, and a page open in headless Chromium reloaded itself with the change.

## `quench build [--release] [--no-embed]`

Without `--release`: the module with `wasm-dev` and the server in debug. With it:

1. the module with the `wasm` profile, copied to `dist/app.wasm`; shrunk with `wasm-opt -Oz` when binaryen is installed (a note says so when it is not);
2. the server with `cargo build --release --features embed`, copied to `dist/<name>-server`.

The binary has the module inside, serves hashed asset names and the strict Content-Security-Policy, and needs no other file: it was run from an empty directory in the tool's acceptance test (page, module and `#[server]` call). With `--no-embed` the module stays a file the server reads from `WASM` (default: the cargo output path), which suits a deployment that serves it from elsewhere.

## `quench build --bundle`

Writes the app as a static bundle in `dist/www` for the Android host: the project's server is run once with `QUENCH_EXPORT=dist/www` and writes `index.html` (the page, rendered on the server side as for a browser) and every hashed asset under `_quench/` (`Ui::export` in [`quench-fullstack`](./quench-fullstack.md)); it then exits instead of serving. The module is the release build. The bundle directory is emptied first, because file names are hashes.

## `quench android init [--package ID]`

Writes `android/`, the Gradle project of the [Android host](./quench-android.md), next to the app (default application id `com.example.<name>`; it refuses to overwrite an existing `android/`). The project is Kotlin with no AndroidX; its Gradle build packages `dist/www` as the app's assets. The Gradle wrapper (9.3.1) is part of it, so `./gradlew assembleDebug` is all a machine with the Android SDK needs.

## `quench plugin add <name>` / `quench plugin list`

Adds a native capability to the project's Android host (`quench android init` first): its Kotlin files, its manifest entries and its line in `plugins()`, and what it needs (`jobs` adds `native`). `list` shows the plugins and which the project has (kept in `android/.quench-plugins`). What each does is in the [Android host](./quench-android.md#plugins).

## `quench android build-native [--release] [--abi ABI]`

Builds the program in `native/` (made by `quench plugin add native`) for arm64-v8a and x86_64 with the NDK's clang as the linker and copies it into `android/app/src/main/jniLibs/<abi>/` as `libquench_native.so`: the host runs it as a separate process, so there is no JNI and no unsafe code in the path. The NDK is `ANDROID_NDK_HOME`, or the newest under the SDK; the Rust targets come from `rustup target add aarch64-linux-android x86_64-linux-android` (`quench doctor` checks them).

## `quench run android [--abi ABI]`

Builds the bundle (and the native program for the device's ABI, found with `adb shell getprop ro.product.cpu.abi`, when the project has one), installs the debug APK with Gradle and starts it on the attached device or emulator. `quench dev --android` is the live version: it starts the app on the device pointing at the dev server (`adb reverse` and `dev_url`, once the server is listening), so an edit reloads the page on the device.

## `quench build apk|aab [--release] [--abi ABI]` and signing

`quench build apk` builds the bundle, the native program if there is one, and a debug APK; `--release` (and always for `aab`) signs with the key `quench.toml` names:

```toml
[android]
package = "dev.lorehaven.demo"     # written by `quench android init`
version_name = "0.1.0"
version_code = 1                   # must go up with every release
keystore = "release.keystore"      # from the project root; keep it out of git
key_alias = "release"
registry = "https://warehouse.example"   # where `quench publish` sends the build
```

`quench android keystore` makes the key with `keytool` and writes `keystore` and `key_alias` into the file. **The passwords are never in a file or on a command line**: `QUENCH_KEYSTORE_PASSWORD` (and `QUENCH_KEY_PASSWORD` if the key has its own) are read from the environment by `keytool`, by `quench`, and by the Gradle build. The result is copied to `dist/<name>-<version_name>-<version_code>.apk` (or `.aab`), and a signed APK is verified with the SDK's `apksigner` (the signer is printed). Measured on this machine: a signed release APK of the counter app is 725 KiB.

## `quench publish [--dry-run] [--apk FILE] [--allow-dirty]`

Builds the signed release APK (or takes `--apk`) and sends it to the warehouse apps are installed and updated from, with the protocol the old per-app scripts used: `PUT <registry>/warehouse/api/v1/artifacts/<package>/android/<version_code>` with a bearer token and the APK as the body; `201` is published, `409` means that version code exists (they are immutable: raise `version_code`). The registry is `WAREHOUSE_URL` or `registry` under `[android]` (there is no built-in default); the token is `WAREHOUSE_TOKEN`, or `WAREHOUSE_USER` and `WAREHOUSE_PASS` to log in through gatehouse. Before anything is sent it checks that the tree is clean (the published build must be traceable to a commit; `--allow-dirty` skips it and the commit is reported with `-dirty`), and that the APK is signed. `--dry-run` does all of that and sends nothing; the tests and the acceptance run publish to a local stand-in server (`scripts/android-fixtures.mjs warehouse`), never the real one.

## `quench doctor`

Checks the machine and prints one line per tool with the command that fixes a gap. Only the web checks are required (the exit code is 1 when one is missing); the rest are shown as `absent` because the commands that need them come with the Android and desktop hosts.

| Check | Needed for |
|---|---|
| `rustc` 1.85+ (edition 2024), `cargo`, the `wasm32-unknown-unknown` target | everything (required) |
| `wasm-opt` | smaller release modules |
| `node` 22+, `chromium` | the browser tests |
| Android SDK, NDK, `adb`, the `aarch64`/`x86_64` Android targets, JDK 17+, `gradle` | the Android host |
| `webkit2gtk-4.1` or `webkitgtk-6.0` (through `pkg-config`) | the Linux desktop host |

The SDK is looked for in `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `~/Android/Sdk` and `/opt/android-sdk`; the NDK in `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT` and the SDK's `ndk/` directory.

## `quench test [--browser] [args...]`

Runs `cargo test --workspace` (the arguments go to it), then `cargo check --target wasm32-unknown-unknown -p <name>-app`: the unit tests run natively, and the check makes sure the browser-only code still compiles, which the native tests cannot tell.

With `--browser` it then builds the module and the server, starts the server on a free port (without the reload script), waits until it listens and runs `tests/browser.mjs` with `node`, with `CHROMIUM` set to the first of `chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable` on `PATH`; the server is stopped whatever the result. The script every new project gets drives headless Chromium over the DevTools protocol (no packages): the server renders the page, the browser takes it over, two clicks change the counter, for fullstack a `#[server]` call answers, and the page raised no exceptions. It is the project's to extend. Without Node 22 or a Chromium the command says what to install before it builds anything.

## Design notes

- Every call to the machine (`run`, `spawn`, `capture`, environment, files, `PATH`) goes through the `System` trait, so the commands are tested with a recording fake and no cargo. `Host` is the real one; a handful of tests run harmless commands through it.
- The project files are in `quench-tool/templates/` as `*.tpl`, rendered by `quench-template` and embedded in the binary. A test renders both templates and checks that no template syntax is left over; the end-to-end acceptance (`quench new`, `quench test`, `quench dev` with a browser, `quench build --release` run from an empty directory) was done by hand against a checkout.
- The versions in `quench-tool/src/versions.rs` are the oldest each generated project needs; a test keeps them on the same minor version as the workspace's crates.
