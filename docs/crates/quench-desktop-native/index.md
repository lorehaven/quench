---
title: quench-desktop-native
parent: Crates
nav_order: 19
has_children: true
permalink: /crates/quench-desktop-native/
---

`quench-desktop-native` runs a `quench_ui_native::NativeRenderer` in a Wayland window. It joins [quench-desktop-wayland](../quench-desktop-wayland/index.md) (window, input) and [quench-desktop-atspi](../quench-desktop-atspi/index.md) (screen readers): it draws a frame whenever the renderer says something changed, turns pointer, wheel, keyboard and input-method events into renderer calls, blinks the caret, repeats held keys, syncs the system clipboard and publishes the accessibility tree.

Version: **0.1.7**. Edition 2024 (workspace). Linux only (`#![cfg(target_os = "linux")]`). No `unsafe`.

## Install

```toml
[dependencies]
quench-desktop-native = { version = "0.1.7", registry = "ennor" }
```

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `quench-desktop-wayland` | `Window`, `WindowEvent`, `Key`, `Modifiers`. |
| `quench-desktop-atspi` | `Service` for accessibility. |
| `quench-ui` | `KeyEvent`. |
| `quench-ui-native` | `NativeRenderer`, `ImeState`. |

Dev-dependencies: `quench-raster`, `quench-test`.

## Where to go next

* [Usage](usage.md), [API reference](api.md), [Behaviour](internals.md), [Testing](testing.md).

## Minimal example

```rust
use std::rc::Rc;
use quench_ui_native::NativeRenderer;

let renderer = Rc::new(NativeRenderer::new());
// renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
quench_desktop_native::run("My app", "dev.example.app", (800, 600), &renderer)?;
# Ok::<(), std::io::Error>(())
```
