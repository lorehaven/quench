# quench-ui-native
A quench-ui renderer that draws itself: a DOM in memory, styled by `quench-css`, laid out by `quench-layout` and painted by `quench-paint`, with no WebView.

## What it is
`quench-ui-native` is a `quench_ui::Renderer` that draws itself. Views are built into an in-memory DOM; `NativeRenderer::render` styles it with `quench-css`, lays it out with `quench-layout` and paints it with `quench-paint` into a `quench-raster` canvas, with text through `quench-text`. There is no WebView and no browser.

A window host only has to show the pixels and forward input. It is the place where a `quench-ui` view lives on platforms without a WebView: Linux/Wayland through `quench-desktop-native` and Android through `quench-android-surface`.

## Use cases
- Run a `quench-ui` app in a native window without a browser.
- Render a view to a canvas for screenshots or tests.
- Drive a frame loop with pointer input, timers and redraw-on-change.
- Tell the page about the host: scale, viewport, color scheme, pointer type and safe-area insets.

## Example
```rust
use std::rc::Rc;
use quench_raster::Canvas;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

let r = Rc::new(NativeRenderer::new());
r.add_stylesheet(&quench_style::components::stylesheet());
r.set_viewport(390.0, 844.0);
let _mounted = mount(&r, &r.root(), my_view());
let mut canvas = Canvas::new(390, 844);
let boxes = r.render(&mut canvas);
```

A host's frame loop forwards input with `pointer_move`, `pointer_down` and `pointer_up`, calls `tick`, redraws when `take_dirty` is true, and sleeps for `next_wake`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-ui-native/
