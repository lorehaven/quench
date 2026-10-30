---
title: Usage
parent: quench-ui-native
nav_order: 1
---

Task-oriented examples. The host (a window, an Android surface, a test) owns the loop; the renderer owns the document.

## 1. Mount a view and draw it

```rust
use std::rc::Rc;
use quench_raster::Canvas;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

let r = Rc::new(NativeRenderer::new());
r.add_stylesheet(&quench_style::components::stylesheet()); // after the built-in browser sheet
r.set_viewport(390.0, 844.0);
let _mounted = mount(&r, &r.root(), my_view());
let mut canvas = Canvas::new(390, 844);
let boxes = r.render(&mut canvas);
```

Effect: the canvas is cleared to the page background (root's, else body's, else white), then painted. `boxes` is the `Rc<LayoutBox>` of the frame. Pitfall: the layout is made for the viewport that was set, not for the canvas size; keep them equal.

## 2. Tell the page about the host

```rust
use quench_ui_native::{ColorScheme, MediaEnv, Pointer};

r.set_scale(2.75);                       // device pixels per CSS pixel (a phone)
r.set_viewport(1080.0, 2400.0);          // device pixels
r.set_media(MediaEnv {
    color_scheme: ColorScheme::Light,
    hover: false,
    pointer: Pointer::Coarse,
    reduced_motion: false,
    ..MediaEnv::default()                // width/height are overwritten from the viewport
});
r.set_safe_area([48.0, 0.0, 63.0, 0.0]); // env(safe-area-inset-*): top, right, bottom, left
```

`@media (max-width: ...)` sees `viewport / scale` CSS pixels. Pointer coordinates and safe-area insets stay in device pixels. Default `MediaEnv` is a 1280x800 desktop with a mouse, dark scheme, motion allowed.

## 3. A frame loop with input, timers and redraw

```rust
use std::time::Duration;

loop {
    // 1. forward input
    r.pointer_move(x, y);
    r.pointer_down(x, y);
    r.pointer_up(x, y);
    // 2. run timers, channels and animation steps
    r.tick();
    // 3. redraw when something changed
    if r.take_dirty() {
        r.render(&mut canvas);
        present(&canvas);
    }
    // 4. sleep until input or the next timer/frame
    let wait: Option<Duration> = r.next_wake();
    wait_for_events(wait);
}
```

`take_dirty` returns `true` once per change. `next_wake` is `None` when nothing will happen without input; with a running transition it is at most 16 ms; with an open channel at most 25 ms.

## 4. Click, hover and focus

```rust
r.pointer_down(10.0, 10.0);
r.pointer_up(10.0, 10.0);   // a Click is delivered to the nearest element containing both ends
```

A click needs press and release on the same element (or on its common ancestor). `:hover`, `:active`, `:focus` and `:focus-visible` follow the pointer and focus. Listeners are those registered through the view (`on_click`, ...). A listener calls `quench_ui::event::prevent_default()` to stop the built-in default (focus, toggle, scroll, Tab, edit). `key_down` returns `true` when a listener prevented the key.

## 5. Typing, clipboard, input methods

```rust
use quench_ui::KeyEvent;

r.pointer_down(fx, fy); r.pointer_up(fx, fy);   // focus a text field
r.text("hello");                                 // typed text
r.key_down(&KeyEvent::new("Backspace"));
r.key_down(&KeyEvent { ctrl: true, ..KeyEvent::new("a") });

// paste: put the system clipboard in, then send the key
r.set_clipboard(&system_clipboard());
r.key_down(&KeyEvent { ctrl: true, ..KeyEvent::new("v") });
// copy/cut: hand the text to the system
if let Some(text) = r.take_clipboard_out() { set_system_clipboard(&text); }

// input method
if let Some(state) = r.ime_state() { place_candidate_window(state.rect); }
r.ime_apply(0, 0, None, Some(("かな", 6)));      // show a composition (cursor in bytes)
r.ime_apply(0, 0, Some("仮名"), None);          // commit
```

The field value lives in the `value` attribute, so a view bound with `attr_fn("value")` and the field agree. `Input` events carry the new value.

## 6. Timers, delays and worker threads

```rust
let tx = r.channel(move |rows: Vec<Row>| rows_signal.set(rows)); // handler runs on the view thread
std::thread::spawn(move || { let _ = tx.send(fetch()); });
// the host ticks; the handler runs inside tick()
r.tick();
```

`quench_ui::platform::after(ms, f)` (toasts, debounces) is backed by the renderer's platform, installed by `NativeRenderer::new()`; due timers run in `tick()`.

## 7. Images

```rust
r.set_image_loader(|src| std::fs::read(src).ok()); // for any src that is not a data: URL
r.add_image("logo.png", &bytes)?;                  // or register bytes up front
```

`<img src="data:image/png;base64,...">` works with no loader. See [pipeline](api-pipeline.md#images).

## 8. Accessibility

```rust
use quench_ui_native::AccessAction;

let tree = r.accessibility_tree();
if let Some(node) = tree.find(some_id) { println!("{:?} {}", node.role, node.name); }
r.perform(some_id, AccessAction::Click);
r.set_field_value(field_id, "new text");
```

## Pitfalls

* Creating a second `NativeRenderer` on the same thread replaces the `quench_ui::platform` installed by the first (timers of the first stop being reachable from `quench_ui::platform::after`).
* Nodes are never freed: a removed node stays in the arena (see [internals](internals.md#node-arena)).
* `render` lays out the viewport, not the canvas. A canvas larger than the viewport shows the page background beyond the document.
* Pointer methods expect coordinates in the same (device pixel) space as `set_viewport`.
