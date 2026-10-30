---
title: quench-android-surface
parent: Crates
nav_order: 2
has_children: true
permalink: /crates/quench-android-surface/
---

`quench-android-surface` is the program side of an Android app drawn by the native renderer (no `WebView`). The Kotlin host owns a `SurfaceView`, the input method and the accessibility services and starts this program as a process of its own; the two talk over the program's standard input and output with a small binary protocol. Touches, keys and text go in; frames, keyboard requests, the focused text field, the accessibility tree and calls to the host's plugins come out.

Version: **0.1.5**. Edition 2024 (workspace). No `unsafe`, no JNI.

## Install

```toml
[dependencies]
quench-android-surface = { version = "0.1.5", registry = "ennor" }
```

The Kotlin side is added to a project with `quench plugin add surface` (see [quench-tool plugins](../quench-tool/plugins.md#surface)).

## Features

None.

## Dependencies

| Crate | Why |
|---|---|
| `quench-raster` | `Canvas` and `Color` for the frame and the selection handles. |
| `quench-serde` | JSON for the plugin bridge envelopes; `Map` and `Value` are re-exported. |
| `quench-ui` | `KeyEvent`. |
| `quench-ui-native` | `NativeRenderer`, `MediaEnv`, `ColorScheme`, `Pointer`, `ImeState`, `AccessNode`, `AccessAction`, `Role`. |

Dev-dependencies: `quench-css`, `quench-test`. No other third-party crates.

## Where to go next

* [Usage](usage.md): the program, plugin calls, driving it from a test.
* [API reference](api.md): `Surface`, the run functions, `proto`, `bridge`.
* [Protocol and behaviour](internals.md): every message byte by byte, the touch state machine, fling, selection, the accessibility tree layout.
* [Testing](testing.md).

## Minimal example

```rust
use std::rc::Rc;
use quench_ui_native::NativeRenderer;

fn main() -> std::io::Result<()> {
    let renderer = Rc::new(NativeRenderer::new());
    // renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
    quench_android_surface::run(&renderer)
}
```
