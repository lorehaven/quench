---
title: Usage
parent: quench-desktop-native
grand_parent: Crates
nav_order: 1
---

*These examples open real windows; the book's checks did not run any.*

## 1. Run a page until the window closes

See the overview. `run` opens the window, draws the first frame and calls `Host::step(1 s)` until the user closes the window.

## 2. Your own loop

```rust
use std::time::Duration;
use quench_desktop_native::Host;

let mut host = Host::open("My app", "dev.example.app", (800, 600), &renderer)?;
while !host.closing() {
    host.step(Duration::from_millis(200))?;   // waits for an event or the next timer
    // your own work here
}
```

`step` waits at most `timeout`, the renderer's next wake (animations), the next caret/repeat timer, and 25 ms while a screen reader has asked something in the last three seconds.

## 3. Test a page without a window

`Controller` owns no window; drive it with `WindowEvent`s:

```rust
use quench_desktop_native::Controller;
use quench_desktop_wayland::WindowEvent;

let mut c = Controller::new(&renderer, (800, 600));
c.handle(WindowEvent::PointerMoved { x: 40.0, y: 40.0 });
c.handle(WindowEvent::PointerButton { x: 40.0, y: 40.0, button: 0x110, pressed: true });
c.handle(WindowEvent::PointerButton { x: 40.0, y: 40.0, button: 0x110, pressed: false });
c.timers(std::time::Instant::now());
```

## 4. What is not handled

Only the left mouse button (`0x110`) reaches the renderer; other buttons are ignored. The window has no HiDPI scale, no cursor shape, and no window decorations beyond what the compositor draws (see [protocol coverage](../quench-desktop-wayland/protocol.md#not-supported)).

## 5. Debugging

`QUENCH_WAYLAND_TRACE=1` also prints each window event the controller handles as `quench-host: <event>` (pointer motion thinned).
