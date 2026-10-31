# quench-desktop-wayland
A Wayland client written against the wire protocol: a window with shared-memory buffers and pointer input, for the native renderer.

## What it is
`quench-desktop-wayland` is a Wayland client with no `libwayland`: a window with two shared-memory buffers that a `quench_raster::Canvas` draws straight into, plus pointer, keyboard and scroll input, key repeat information, an input method (text-input v3) and the clipboard. It replaces `wayland-client` and `winit` for the native renderer.

It depends on `libc` and `quench-raster`. `unsafe` is confined to `src/sys.rs` (file-descriptor passing, `memfd_create`, `mmap`). Linux only; the crate is empty on other targets. `quench-desktop-native` builds on it.

## Use cases
- Open a Wayland window and draw into it with a `Canvas`.
- Read pointer, keyboard and scroll events through `WindowEvent`.
- Run host timers such as caret blink using `next_event` with a timeout.
- Copy to and paste from the system clipboard.
- Support input methods (IME) in text fields.

## Example
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

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-desktop-wayland/
