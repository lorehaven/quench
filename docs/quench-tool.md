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
