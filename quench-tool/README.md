# quench-tool
The `quench` command: new projects, a dev loop with live reload, release builds, doctor and tests for quench-ui apps.

## What it is
`quench-tool` provides the `quench` command: one command per step of building a `quench-ui` app, in the way of the `flutter` CLI. It creates projects, runs a dev loop with browser reload, builds release binaries, bundles and packages (APK, AAB, `.deb`, `.rpm`, Arch, source tarball), generates the Android and Linux desktop hosts, manages the Android plugins and checks the machine.

It runs `cargo`, `gradle` (the project's wrapper), `keytool`, `adb` and `node`; it has no build system of its own. The commands are `new`, `dev`, `build`, `run`, `linux`, `android`, `plugin`, `doctor` and `test`.

## Use cases
- Scaffold a `quench-ui` web or fullstack project.
- Develop with a rebuild-and-reload loop, optionally on an Android device or a Linux desktop host.
- Produce release builds and distributable packages.
- Check that the machine has the required toolchain.
- Run a project's tests, including a wasm32 check.

## Example
```sh
cargo install quench-tool --registry ennor      # the binary is `quench`

quench doctor                      # required: rustc 1.85+, cargo, wasm32 target
quench new demo --template fullstack
cd demo
quench dev --open                  # http://127.0.0.1:3000, reloads on every edit
quench test                        # cargo test --workspace, then cargo check for wasm32
quench build --release             # dist/app.wasm and dist/demo-server
```

Project settings live in `.quench.toml`, which `quench new` writes with `name` and `template`:

```toml
name = "demo"
template = "fullstack"
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-tool/
