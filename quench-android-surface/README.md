# quench-android-surface
The program side of an Android app drawn by the native renderer: touch, key and text events in, frames and accessibility out.

## What it is
`quench-android-surface` is the program side of an Android app drawn without a `WebView`. The Kotlin host owns a `SurfaceView`, the input method and the accessibility services and starts this program as a process of its own; the two talk over standard input and output with a small binary protocol. Touches, keys and text go in; frames, keyboard requests, the focused text field, the accessibility tree and calls to the host's plugins come out.

It sits on `quench-ui-native` (`NativeRenderer`), `quench-raster` (`Canvas`), `quench-ui` and `quench-serde`. The Kotlin side is added with `quench plugin add surface` from `quench-tool`. No `unsafe`, no JNI.

## Use cases
- Run a `quench-ui-native` view as an Android app.
- Call the host's Kotlin plugins from the program through a `Bridge`.
- Drive the program in tests over in-memory streams with `serve`.
- Inspect frame timing with the `QUENCH_FRAME_STATS` environment variable.

## Example
```rust
use std::rc::Rc;
use quench_ui_native::NativeRenderer;

fn main() -> std::io::Result<()> {
    let renderer = Rc::new(NativeRenderer::new());
    // renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
    quench_android_surface::run(&renderer)
}
```

Calling a host plugin:

```rust
use quench_android_surface::{Bridge, Value};

let bridge = Bridge::new();
bridge.call("device", "state", Value::Null, |answer| match answer {
    Ok(state) => eprintln!("online: {:?}", state.get("online")),
    Err(e) => eprintln!("failed: {e}"),
});
quench_android_surface::run_with(&renderer, &bridge)?;
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-android-surface/
