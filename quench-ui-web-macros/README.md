# quench-ui-web-macros
The `#[plugin]` attribute for `quench-ui-web`: a typed client for a native plugin of the host app.

## What it is
`quench-ui-web-macros` is a proc-macro crate that provides the `#[plugin]` attribute. You write a trait of `async fn` declarations and the macro turns it into a unit struct with the same name, a `NAME` constant and one associated async function per method, each calling the host plugin through the bridge.

Depend on `quench-ui-web`, which re-exports it as `quench_ui_web::plugin`; the expansion refers to `::quench_ui_web`, so it only compiles where that crate is a dependency.

## Use cases
- Call an Android or desktop host plugin from wasm code with typed arguments and results.
- Declare a client for a custom plugin by its name.
- Return structured data from the host through types that derive `Serialize` and `Deserialize`.

## Example
```rust
use quench_serde::{Deserialize, Serialize};
use quench_ui_web::BridgeError;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Device { sdk: u32, model: String }

#[quench_ui_web::plugin("echo")]
pub trait Echo {
    /// Returns the text.
    async fn say(text: String) -> Result<String, BridgeError>;
    async fn device() -> Result<Device, BridgeError>;
    async fn add(a: i64, b: i64) -> Result<i64, BridgeError>;
}

// let reply = Echo::say("hi".into()).await?;
```

The plugin name must start with an ASCII lowercase letter and contain only lowercase letters, digits and `_`.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-ui-web-macros/
