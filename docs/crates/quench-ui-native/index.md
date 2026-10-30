---
title: quench-ui-native
parent: Crates
nav_order: 57
has_children: true
---

`quench-ui-native` (crate `quench_ui_native`) is a [`quench_ui::Renderer`](../quench-ui/api-render.md) that draws itself. Views are built into an in-memory DOM; [`NativeRenderer::render`](api-pipeline.md#render) styles it with [quench-css](../quench-css/index.md), lays it out with [quench-layout](../quench-layout/index.md), paints it with [quench-paint](../quench-paint/index.md) into a [quench-raster](../quench-raster/index.md) canvas (text through [quench-text](../quench-text/index.md)). There is no WebView and no browser. A window host only has to show the pixels and forward input.

It replaces the browser as the place where a `quench-ui` view lives, on platforms where the framework runs without a WebView (Linux/Wayland through `quench-desktop-native`, Android through `quench-android-surface`).

* Version: 0.1.5. Edition 2024 (workspace). No `rust-version`.
* No `unsafe` code (the workspace sets `unsafe_code = "deny"`; `src/` contains none).
* Thread model: everything is `!Send`/`!Sync` (`Rc`, `RefCell`). One renderer lives on one thread. Only the `Sender` returned by [`channel`](api-timers.md#channel) crosses threads.
* The workspace denies `missing_debug_implementations`; `NativeRenderer` implements `Debug` by hand (prints only the node count). Clippy allowances for float arithmetic are at the top of `src/lib.rs`.

## Install

```toml
[dependencies]
quench-ui = { version = "0.1", registry = "ennor" }
quench-ui-native = { version = "0.1", registry = "ennor" }
quench-raster = { version = "0.1", registry = "ennor" }
```

(The workspace declares `quench-ui-native = { path = "quench-ui-native", registry = "ennor", version = "0.1.0" }`.)

## Features

None. The crate has no `[features]` section and no optional dependencies.

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | Base64 decoding of `data:` URLs (`quench_core::base64::STANDARD`) |
| `quench-css` | Parsing sheets, the cascade, `ComputedStyle`, `MediaEnv`, the `Element` trait the DOM implements |
| `quench-layout` | `layout`, `LayoutBox`, `Node`, `Fonts` (also used for text measuring and caret geometry) |
| `quench-paint` | `paint`, `Decoration`, `Layers` (hit-testing order) |
| `quench-raster` | `Canvas`, `Color` |
| `quench-image` | Decoding PNG, JPEG, GIF and WebP for `<img>` and `background-image` |
| `quench-svg` | Drawing SVG images at the page density |
| `quench-ui` | The `Renderer` trait, `Event`/`KeyEvent`/`PointerEvent`, the `platform` hook |

Dev-dependencies: `quench-style` (the component stylesheet), `quench-test` (the test harness).

## Re-exports

`pub use quench_css::{ColorScheme, MediaEnv, Pointer};` so a host can build a `MediaEnv` without depending on `quench-css`. See [api-dom](api-dom.md#re-exports).

## Where to go next

* [Usage](usage.md): mounting, driving a frame loop, input, timers, accessibility, with worked examples.
* [DOM API](api-dom.md): `NativeRenderer` construction, host settings, DOM inspection, the `Renderer` impl, `NodeId`, `NodeKind`.
* [Pipeline API](api-pipeline.md): `layout`, `render`, `UA_STYLESHEET`; what each stage does here and which CSS the renderer itself reads.
* [Input API](api-input.md): every pointer, wheel, key, text, IME, clipboard and selection call and what it does.
* [Access API](api-access.md): `accessibility_tree`, `perform`, `AccessNode`, `Role`, the role/attribute mapping.
* [Animation API](api-anim.md): transitions and keyframe animations, supported properties.
* [Timers API](api-timers.md): `tick`, `next_wake`, `channel`, `advance_animations`, the platform clock.
* [Internals](internals.md): dirty flags, layout reuse, id scheme, limits.
* [Testing](testing.md): the unit suite and the Chromium gate.

## Minimal example

Taken from the shape of the crate's own tests (`tests/unit.rs`, `page` and `live` helpers):

```rust
use std::rc::Rc;
use quench_raster::Canvas;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

let r = Rc::new(NativeRenderer::new());
r.add_stylesheet("body { margin: 0 } .box { width: 40px; height: 30px; background: red }");
r.set_viewport(100.0, 100.0);
let _mounted = mount(&r, &r.root(), div().class("box")); // root() is <body>
let mut canvas = Canvas::new(100, 100);
let boxes = r.render(&mut canvas);                      // style, layout, paint
assert_eq!(canvas.pixel(5, 5), quench_raster::Color::rgb(255, 0, 0));
let _ = boxes;                                          // the layout, also kept inside
```

Keep the value returned by `mount` alive for as long as the view should stay (the tests call `std::mem::forget` on it when they do not need it afterwards).
