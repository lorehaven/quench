---
title: quench-desktop
parent: Crates
nav_order: 17
has_children: true
permalink: /crates/quench-desktop/
---

`quench-desktop` is the Linux desktop host for a Quench UI bundle: a GTK 3 window around webkit2gtk 4.1 that serves the app's assets from its own origin (`quench://app/`) and carries the same plugin bridge the Android host has. It replaces Electron and Tauri for Quench apps. The generated `desktop/` crate (`quench linux init`) is a few lines around it.

Version: **0.1.0**. Edition 2024 (workspace). Linux only (`#![cfg(target_os = "linux")]`).

The parts that need no GTK are always built: the bundle and its routing, the bridge, the plugins, the origin rules. The window itself (`ffi`, `host`, the control socket) needs the `window` feature and, to link, the system libraries `gtk-3` and `webkit2gtk-4.1`.

`unsafe` is confined to `src/ffi.rs` (the calls into GTK 3, webkit2gtk and GLib), with a safety comment per block; the safe types it exposes are `!Send` and `!Sync` and can only be used on the thread that initialised GTK.

## Install

```toml
[dependencies]
quench-desktop = { version = "0.1.0", registry = "ennor", features = ["window"] }
```

## Features

| Feature | Default | Effect |
|---|---|---|
| `window` | off | Builds `ffi` (`Gtk`, `Remote`, `WebView`, `Window`), `host` (`App`, `RunError`, `run`) and `control`. Needs GTK 3 and webkit2gtk 4.1 development libraries to link. Required by the test fixture binary `quench-desktop-fixture`. |

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | base64 for `files.read`; temp dirs in tests. |
| `quench-serde` | JSON for the bridge envelopes and the control socket. |

Dev-dependency: `quench-test`. No GTK crates: the GTK/webkit calls are declared by hand in `ffi.rs`.

## Where to go next

* [Usage](usage.md): an app, plugins, a custom plugin, testing without a window.
* [API reference](api.md).
* [Plugins](plugins.md): `echo`, `device`, `secure_storage`, `notifications`, `share`, `files` on the desktop.
* [Internals](internals.md): origin rules, security, the control socket, threading.
* [Configuration](configuration.md), [Testing](testing.md).
* Guide: [Desktop and Android hosts](../../guides/hosts.md).

## Minimal example

This is what `quench linux init` generates (`src/main.rs`):

```rust
include!(concat!(env!("OUT_DIR"), "/bundle.rs"));   // fn bundle() -> quench_desktop::Bundle

fn main() -> std::process::ExitCode {
    let app = quench_desktop::App::new("My app", bundle()).standard_plugins("com.example.my-app");
    match quench_desktop::run(app) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => { eprintln!("my-app: {error}"); std::process::ExitCode::FAILURE }
    }
}
```
