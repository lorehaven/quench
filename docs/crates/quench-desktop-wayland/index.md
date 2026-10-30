---
title: quench-desktop-wayland
parent: Crates
nav_order: 20
has_children: true
permalink: /crates/quench-desktop-wayland/
---

`quench-desktop-wayland` is a Wayland client written against the wire protocol, with no `libwayland`: a window with two shared-memory buffers that a `quench_raster::Canvas` draws straight into, pointer, keyboard and scroll input, key repeat information, an input method (text-input v3) and the clipboard. It replaces `wayland-client` and `winit` for the native renderer.

Version: **0.1.5**. Edition 2024 (workspace). Linux only (`#![cfg(target_os = "linux")]`: the crate is empty on other targets).

`unsafe` is confined to `src/sys.rs` (file-descriptor passing with `sendmsg`/`recvmsg`, `memfd_create`, `mmap`), which carries `#![allow(unsafe_code)]` and a safety comment per block. Everything else is safe code.

## Install

```toml
[dependencies]
quench-desktop-wayland = { version = "0.1.5", registry = "ennor" }
```

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `libc` | `sendmsg`, `recvmsg`, `memfd_create`, `ftruncate`, `mmap`, `munmap` in `sys`. |
| `quench-raster` | `Canvas`, the drawing surface over the shared memory. |

Dev-dependencies: `quench-core`, `quench-test`. The test fixture binary `quench-desktop-wayland-fixture` (`tests/fixture/main.rs`) uses `quench-raster`.

## Environment

| Variable | Read by | Meaning |
|---|---|---|
| `WAYLAND_DISPLAY` | `Socket::connect_display` | Socket name under `$XDG_RUNTIME_DIR`, or an absolute path. Default `wayland-0`. |
| `XDG_RUNTIME_DIR` | `Socket::connect_display` | Directory of the socket; if missing and the name is relative: error `XDG_RUNTIME_DIR is not set`. |
| `QUENCH_WAYLAND_TRACE` | `trace_enabled` | If set (any value), the window prints what the compositor sent and what it made of it to standard error. |

## Where to go next

* [Usage](usage.md): opening a window, drawing, events, clipboard, input method.
* [API reference](api.md): every public item by module.
* [Protocol coverage](protocol.md): the interfaces, versions, requests and events this client speaks, and what it does not.
* [Internals](internals.md): buffers, configure handshake, keymap parsing, limits.
* [Testing](testing.md).

## Minimal example

```rust
use quench_desktop_wayland::{Window, WindowEvent};
use quench_raster::Color;

fn main() -> std::io::Result<()> {
    let mut window = Window::open("Hello", "dev.example.hello", 640, 400)?;
    loop {
        let mut canvas = window.begin_frame()?;
        canvas.clear(Color::rgb(30, 32, 38));
        drop(canvas);
        window.present()?;
        if let Some(WindowEvent::CloseRequested) = window.next_event(None)? {
            break;
        }
    }
    Ok(())
}
```
