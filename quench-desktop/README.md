# quench-desktop
The Linux desktop host: a GTK window around webkit2gtk that serves an app's embedded assets and carries its plugin bridge.

## What it is
`quench-desktop` is the Linux desktop host for a Quench UI bundle: a GTK 3 window around webkit2gtk 4.1 that serves the app's assets from its own origin (`quench://app/`) and carries the same plugin bridge the Android host has. It replaces Electron and Tauri for Quench apps. The generated `desktop/` crate (`quench linux init`) is a few lines around it.

Parts that need no GTK (the bundle and its routing, the bridge, the plugins) are always built; the window needs the `window` feature and the `gtk-3` and `webkit2gtk-4.1` libraries to link. It depends on `quench-core` and `quench-serde`. The GTK calls are declared by hand, with `unsafe` confined to `src/ffi.rs`.

## Use cases
- Ship a Quench web UI as a native Linux desktop app.
- Embed the built assets in the binary and serve them from `quench://app/`.
- Add the standard plugins (device, secure storage, notifications, share, files).
- Write a plugin of your own and answer calls from the page.
- Point the window at a dev server in debug builds.

## Example
This is what `quench linux init` generates (`src/main.rs`):

```rust
include!(concat!(env!("OUT_DIR"), "/bundle.rs"));   // fn bundle() -> quench_desktop::Bundle

fn main() -> std::process::ExitCode {
    let app = quench_desktop::App::new("My app", bundle()).standard_plugins("com.example.my-app");
    match quench_desktop::run(app) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => { eprintln!("my-app: {error}"); std::process::ExitCode::FAILURE }
    }
}
```

```toml
[dependencies]
quench-desktop = { version = "0.1.0", registry = "ennor", features = ["window"] }
```

## Features
- `window` (off by default): builds the GTK and webkit2gtk window, `App`, `run` and the control socket.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-desktop/
