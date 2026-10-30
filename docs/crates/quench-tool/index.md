---
title: quench-tool
parent: Crates
nav_order: 54
has_children: true
permalink: /crates/quench-tool/
---

`quench-tool` provides the `quench` command: one command per step of building a `quench-ui` app, in the way of the `flutter` CLI. It creates projects, runs a dev loop with browser reload, builds release binaries, bundles and packages (APK, AAB, `.deb`, `.rpm`, Arch, source tarball), generates the Android and Linux desktop hosts, manages the Android plugins and checks the machine. It runs `cargo`, `gradle` (the project's wrapper), `keytool`, `adb` and `node`; it has no build system of its own.

Version: **0.1.13** (`quench --version` prints `quench 0.1.13`). Edition 2024 (workspace). The package has a library (`quench_tool`) and the binary `quench` (`src/main.rs`: `std::process::exit(quench_tool::run(std::env::args_os().skip(1)))`).

## Install

```bash
cargo install quench-tool --registry ennor      # the binary is `quench`
```

`anvil release` installs it on the machine that releases it (it is under `[install]` in the quench repository's `.anvil.toml`; the forge toolbox lists it).

## Dependencies

| Crate | Why |
|---|---|
| `quench-args` | the command line (`Parser`, `Subcommand`, `ValueEnum`). |
| `quench-template` | renders the project and host files (`*.tpl` embedded in the binary). |
| `quench-serde` | reads `.quench.toml` (`Deserialize`, `toml`) and builds template contexts (`json!`). |
| `quench-archive` | `tar`, `gzip`, `zstd` for the `.deb`, Arch package and source tarball. |
| `quench-crypto` | SHA-256 for the RPM digests. |
| `quench-client` (features `blocking`, `tls`) | the `System::http` method. The commands do not call it (see the gap report). |

Dev-dependency: `quench-test`. No other third-party crates. No features.

## Pages

* [Commands](commands.md): every command and subcommand, flags, exit codes, examples.
* [Configuration](configuration.md): `.quench.toml` (every key), environment variables, files the tool writes.
* [Android plugins](plugins.md): the eleven plugins, their bridge operations, JSON shapes, permissions and Kotlin classes.
* [Recipes](recipes.md): end-to-end workflows.
* [API reference](api.md): the library's public items.
* [Internals](internals.md): the generated project, `System`, templates, markers.
* [Testing](testing.md).

Guides: [Packaging apps](../../guides/packaging.md) and [Desktop and Android hosts](../../guides/hosts.md).

## Global options and exit codes

`quench --help` / `-h` and `quench --version` / `-V` print to standard output and exit 0 (so does `--help` on any subcommand). There are no other global options and no environment fallbacks for flags.

| Exit code | Meaning |
|---|---|
| 0 | success (including help and version) |
| 1 | the command failed: `error: <message>` on standard error; `quench doctor` also exits 1 when a required check is missing |
| 2 | the command line could not be parsed (unknown subcommand, bad value); the parser's message is on standard error |

Every command except `new` and `doctor` first finds the project: it looks for `.quench.toml` in the current directory and then in each parent. If there is none it fails with `no .quench.toml in <dir> or above it: run this inside a project made by `quench new``.

## Concepts

* A **project** is a directory with `.quench.toml` and the crates `app/` (UI, built for the browser as wasm), `core/` (logic), `server/`, and for the `fullstack` template `shared/`. Packages are named `<name>-app`, `<name>-core`, `<name>-server`, `<name>-shared`. See [internals](internals.md#generated-projects).
* A **host** wraps the built **bundle** (`dist/www`: `index.html` and hashed files under `_quench/`) in a shell: `android/` (a Gradle project, Kotlin, a `WebView`) or `desktop/` (a Rust crate, GTK and webkit2gtk).
* **Plugins** give the web app native capabilities through a JSON bridge. Android plugins are Kotlin files added by `quench plugin add`; desktop plugins are Rust types in `quench-desktop`.
* **Flat layout:** a project may live inside a larger Cargo workspace; the tool finds the enclosing workspace (the nearest `Cargo.toml` above the project with a line `[workspace]`) and builds into its `target/`, tests only the project's crates and edits that workspace's `members` and `[workspace.dependencies]`.
