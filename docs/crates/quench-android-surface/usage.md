---
title: Usage
parent: quench-android-surface
grand_parent: Crates
nav_order: 1
---

## 1. A program that draws a view

```rust
use std::rc::Rc;
use quench_ui_native::NativeRenderer;

let renderer = Rc::new(NativeRenderer::new());
// renderer.add_stylesheet(css);
// mount(&renderer, &renderer.root(), view);
quench_android_surface::run(&renderer)?;
```

`run` blocks until the host closes the pipe (or the page leaves through Back). Standard output belongs to the protocol; log with `eprintln!` (the Kotlin host sends standard error to logcat).

## 2. Calling the host's plugins

One `Bridge` is shared by the views and the run loop; clones share state, and it is single-threaded (`Rc<RefCell<..>>`, not `Send`).

```rust
use quench_android_surface::{Bridge, Value};

let bridge = Bridge::new();
bridge.call("device", "state", Value::Null, |answer| match answer {
    Ok(state) => eprintln!("online: {:?}", state.get("online")),
    Err(e) => eprintln!("failed: {e}"),   // "<code>: <message>"
});
bridge.on("device", "state", |data| eprintln!("changed: {data:?}"));
quench_android_surface::run_with(&renderer, &bridge)?;
```

Calls are queued and written to the host on the next `flush` of the loop. The host forwards them to the same Kotlin plugins a web page uses (see [plugins](../quench-tool/plugins.md)). The closure runs on the loop's thread when the answer arrives.

Pitfall: a call made before `run_with` starts is queued and goes out on the first loop iteration; an answer for an unknown id is ignored.

## 3. Driving a program over in-memory streams

`serve` takes any `Read + Send + 'static` and `Write`; this is how the tests run it.

```rust
use quench_android_surface::proto::{Event, encode_event};

let mut input = Vec::new();
input.extend(encode_event(&Event::Resize { width: 800, height: 600, density: 2.0, insets: [0.0; 4] }));
input.extend(encode_event(&Event::Touch { action: 0, x: 40.0, y: 40.0 }));
input.extend(encode_event(&Event::Touch { action: 2, x: 40.0, y: 40.0 }));
let mut output: Vec<u8> = Vec::new();
quench_android_surface::serve(&renderer, std::io::Cursor::new(input), &mut output)?;
// `output` now holds length-prefixed messages: a frame (kind 1), the tree (kind 4), ...
```

When the input ends the loop stops (`Closed`). A `&mut Vec<u8>` works as the writer.

## 4. Frame timing

Set `QUENCH_FRAME_STATS` (any value) in the program's environment to print, once a second, `frames: N/s WxH  render .. ms  convert .. ms  send .. ms  (per frame)` to standard error.

## 5. Reading frames on the host

A frame message (kind 1) is `width`, `height` and `width*height*4` bytes in RGBA order (the program swaps the canvas' BGRA in place before sending). See [internals](internals.md#program-to-host).
