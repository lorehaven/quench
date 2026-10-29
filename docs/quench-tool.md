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

Install it with `cargo install quench-tool --registry ennor`; the binary is `quench`. `anvil release` installs it on the machine that releases it (`quench-tool` is under `[install]` in `.anvil.toml`, as forge's own tools are), and `forge-toolbox` lists it with the other installables, so it shows installed against latest and updates with a keypress.

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

The root `.quench.toml` (`name`, `template`) is how the other commands recognise a project; they find it from any directory below. The workspace defines two wasm profiles: `wasm-dev` (quick to compile, used by `dev` and a plain `build`) and `wasm` (size-optimised: `opt-level = "z"`, LTO, `panic = "abort"`, stripped). Without `--quench-path` the project depends on the registry `ennor` and gets a `.cargo/config.toml` naming it.

**Two layouts.** By default a project is a cargo workspace of its own (`quench new` writes the root `Cargo.toml`). A project can also sit *inside* a larger workspace, as a service among a repository's others (the **flat** layout): leave the project's own `Cargo.toml` out, list its crates (`app`, `core`, `desktop`, ...) among the repository workspace's `members`, give each its own `version`, and keep `.quench.toml` in the project's directory. The tool finds the enclosing workspace by itself (the nearest `Cargo.toml` above with a `[workspace]`) and then builds into *its* `target/`, tests the project's own crates by name instead of `--workspace`, and edits that workspace's `members` and `[workspace.dependencies]` (with the project's directory in the path) when `quench linux init` or `quench plugin add` adds a crate. The version of a project with no root manifest comes from `desktop/Cargo.toml`.

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

## `quench linux init [--id ID] [--quench-path DIR]`

Writes `desktop/`, the host crate of the [Linux desktop host](./quench-desktop.md): a `Cargo.toml`, a `main.rs` that shows the bundle with the standard plugins, and a `build.rs` that embeds `dist/www`. In a project with a workspace it joins it (a member, and `quench-desktop` among the workspace dependencies, from the same checkout or registry as the other quench crates); in a directory with no `Cargo.toml` it is a crate of its own, depending on the checkout named by `--quench-path` or on the registry. The application id (`--id`, default `com.example.<name>`) is written to a `[linux]` table in `.quench.toml` with an optional `title`; it names the app to the keyring, the notification service and the menu entry. It refuses to overwrite an existing `desktop/`.

## `quench build linux [--release]`, `quench build deb|rpm|arch|source`, `quench run linux`, `quench dev --linux`

`build linux` makes the bundle (as `build --bundle` does), builds the host crate and copies the binary to `dist/<name>-linux`; with `--release` it is optimised. The bundle is embedded, so the binary runs from an empty directory with no network. `build deb` builds the release host and writes `dist/<name>_<version>_amd64.deb` (the binary in `/usr/bin`, a menu entry; the version is the workspace's). `build rpm` writes `dist/<name>-<version>-1.x86_64.rpm`, `build arch` writes `dist/<name>-<version>-1-x86_64.pkg.tar.zst` (for `pacman -U`), and `build source` writes `dist/<name>-<version>.tar.gz`, the project's sources to build from (without `target/`, `dist/`, build output, signing keys; a project inside a larger workspace brings that workspace's `Cargo.toml`, cut down to its crates, and `Cargo.lock`). The packages hold the same two files as the `.deb` and are written by hand from their formats (the RPM has the version 4 headers, a SHA-256 of the header and of the payload; nothing is signed); `rpm`, `dnf`, `pacman` and `dpkg` install them. `[linux]` in `.quench.toml` takes `id`, `title`, `bin`, `icon` (a square PNG, from the project root, installed in the `hicolor` theme and named by the menu entry), `wm_class` (the menu entry's `StartupWMClass`, for a native window its application id) and `depends` (the `.deb`'s `Depends:`; the default is a web host's WebKit and GTK, `""` for none). `run linux` builds the debug host and runs it. `dev --linux` is the live version: once the dev server listens it opens the window on it (`QUENCH_DEV_URL`, debug builds only) and keeps the window across server restarts, so an edit reloads the page in it.

## `quench run android [--abi ABI]`

Builds the bundle (and the native program for the device's ABI, found with `adb shell getprop ro.product.cpu.abi`, when the project has one), installs the debug APK with Gradle and starts it on the attached device or emulator. `quench dev --android` is the live version: it starts the app on the device pointing at the dev server (`adb reverse` and `dev_url`, once the server is listening), so an edit reloads the page on the device.

## `quench build apk|aab [--release] [--abi ABI]` and signing

`quench build apk` builds the bundle, the native program if there is one, and a debug APK; `--release` (and always for `aab`) signs with the key `.quench.toml` names:

```toml
[android]
package = "dev.lorehaven.demo"     # written by `quench android init`
version_name = "0.1.0"
version_code = 1                   # must go up with every release
keystore = "release.keystore"      # from the project root; keep it out of git
key_alias = "release"
```

`quench android keystore` makes the key with `keytool` and writes `keystore` and `key_alias` into the file. **The passwords are never in a file or on a command line**: `QUENCH_KEYSTORE_PASSWORD` (and `QUENCH_KEY_PASSWORD` if the key has its own) are read from the environment by `keytool`, by `quench`, and by the Gradle build. The result is copied to `dist/<name>-<version_name>-<version_code>.apk` (or `.aab`), and a signed APK is verified with the SDK's `apksigner` (the signer is printed). Measured on this machine: a signed release APK of the counter app is 725 KiB.

## Publishing

The tool builds and never publishes: it has no idea where a release goes. `quench build apk --release` (signed), `quench build aab --release`, `quench build linux`, `deb`, `rpm`, `arch` and `source` make the packages in `dist/`; **`anvil artifact release`** (in the forge repository) builds them and sends them to the warehouse. A project's version is its Cargo version (`[workspace.package]`, else the desktop crate's); the Android `versionName` is that, and the `versionCode` is `major * 1_000_000 + minor * 1_000 + patch`, so `0.1.3` is `1003` on every platform. `version_name` and `version_code` under `[android]` in `.quench.toml` still override it.

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
