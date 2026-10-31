# quench-desktop-native
Runs a `quench-ui-native` renderer in a Wayland window.

## What it is
`quench-desktop-native` runs a `quench_ui_native::NativeRenderer` in a Wayland window. It joins `quench-desktop-wayland` (window, input) and `quench-desktop-atspi` (screen readers): it draws a frame whenever the renderer says something changed, turns pointer, wheel, keyboard and input-method events into renderer calls, blinks the caret, repeats held keys, syncs the system clipboard and publishes the accessibility tree.

`run` opens the window and loops until it closes; `Host` gives a step-by-step loop of your own, and `Controller` takes `WindowEvent`s without any window, for tests. Linux only.

## Use cases
- Show a native-renderer page in a Wayland window with one call.
- Drive your own event loop with `Host::step` alongside other work.
- Test a page's input handling without a window, using `Controller`.
- Get screen-reader support through AT-SPI without extra wiring.

## Example
```rust
use std::rc::Rc;
use quench_ui_native::NativeRenderer;

let renderer = Rc::new(NativeRenderer::new());
// renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
quench_desktop_native::run("My app", "dev.example.app", (800, 600), &renderer)?;
# Ok::<(), std::io::Error>(())
```

```rust
use std::time::Duration;
use quench_desktop_native::Host;

let mut host = Host::open("My app", "dev.example.app", (800, 600), &renderer)?;
while !host.closing() {
    host.step(Duration::from_millis(200))?;   // waits for an event or the next timer
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-desktop-native/
