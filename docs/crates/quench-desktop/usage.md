---
title: Usage
parent: quench-desktop
grand_parent: Crates
nav_order: 1
---

*Examples that open a window were not run for this book.*

## 1. Show a bundle

```rust
use quench_desktop::{App, Bundle, run};

let bundle = Bundle::from_dir(std::path::Path::new("dist/www"))?;   // or Bundle::new().with("index.html", &b"..."[..])
run(App::new("Notes", bundle))?;                                     // blocks until the window closes
# Ok::<(), Box<dyn std::error::Error>>(())
```

`run` initialises GTK (`RunError::NoDisplay` without a display), creates a 1000 x 700 window, registers the `quench:` URI scheme, loads `quench://app/` and runs the GTK main loop. `Bundle::with(path, bytes)` accepts embedded bytes (`include_bytes!`) without copying; `quench linux init` writes a `build.rs` that does this for every file of `dist/www`.

## 2. Plugins

```rust
let app = App::new("Notes", bundle)
    .standard_plugins("dev.lorehaven.notes")   // device, secure_storage, notifications, share, files
    .with_echo();                              // the `echo` demo plugin (debug builds only, by convention)
```

`standard_plugins(id)`: `id` names the app to the keyring and, with the window title, to the notification service.

## 3. A plugin of your own

```rust
use quench_desktop::{Call, Plugin, string_arg};
use quench_serde::{Value, json};

struct Clock;
impl Plugin for Clock {
    fn name(&self) -> &'static str { "clock" }
    fn call(&self, method: &str, args: &Value, call: Call) {
        match method {
            "now" => call.resolve(&json!(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64))),
            "echo" => match string_arg(args, "text") {
                Ok(t) => call.resolve(&json!(t)),
                Err(m) => call.reject("invalid_args", &m),
            },
            _ => call.reject("unknown_method", &format!("clock has no method `{method}`")),
        }
    }
}
let app = App::new("x", bundle).plugin(Clock);
```

A call may be answered later and from any thread (`Call` is `Send` through the `Deliver` it holds). Events: override `Plugin::attach(&self, emitter: Emitter)` and keep the emitter; `emitter.emit("clock", "tick", &json!(1))` reaches the page as `{"event","plugin","data"}`.

## 4. Dev mode

```rust
let app = App::new("Notes", bundle).dev("http://127.0.0.1:3000");   // sets dev_url and debug = true
```

The window then loads the dev server instead of the bundle and allows it as an app origin. The `debug` flag turns on the web inspector. The generated `main.rs` does this in debug builds when `QUENCH_DEV_URL` is set (`quench dev --linux` sets it).

## 5. Test without a window

The bundle router, the bridge and the plugins need no GTK. `Bundle::answer("quench://app/x.js")` gives an `Answer`; `Bridge::new(plugins, deliver)` plus `bridge.handle(json_text)` runs a call and passes the reply JSON to `deliver`:

```rust
use std::sync::{Arc, Mutex};
let replies = Arc::new(Mutex::new(Vec::<String>::new()));
let sink = Arc::clone(&replies);
let bridge = quench_desktop::Bridge::new(
    vec![Arc::new(quench_desktop::plugins::Echo)],
    Arc::new(move |text| sink.lock().unwrap().push(text)),
);
bridge.handle(r#"{"id":1,"plugin":"echo","method":"add","args":{"a":2,"b":3}}"#);
// replies: [{"id":1,"ok":5}]
```

Plugins that run system programs take a `Runner`; give them a stand-in to see exactly what would run (`SecureStorage::new(app, runner)`, `Notifications::new(name, runner)`, `Share::new(runner)`).

## Pitfalls

* Run `run` on the main thread; GTK types are `!Send`.
* A page not on the app's origin gets no answers and its messages are refused (`quench-desktop: refused origin <origin>` on standard error).
* A link out of the app opens in the system's browser (`xdg-open`) only for `https://`, `http://` and `mailto:`; every other navigation away from the app is blocked silently.
