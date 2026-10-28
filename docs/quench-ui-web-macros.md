# Quench UI Web Macros

`quench-ui-web-macros` (crate `quench_ui_web_macros`) provides `#[plugin]`, re-exported as `quench_ui_web::plugin`; depend on `quench-ui-web` rather than on this crate. What the calls are and how they travel is in [`quench-ui-web`](./quench-ui-web.md#the-host-bridge); the host's side is in the [Android host](./quench-android.md#the-bridge).

```rust
use quench_ui_web::BridgeError;

#[quench_ui_web::plugin("echo")]
pub trait Echo {
    /// Returns the text it was given.
    async fn say(text: String) -> Result<String, BridgeError>;
    async fn device() -> Result<Device, BridgeError>;
    async fn add(a: i64, b: i64) -> Result<i64, BridgeError>;
}

let reply = Echo::say("hi".into()).await?;
```

The trait is the contract of the plugin on the Rust side. It becomes a unit struct `Echo` (with `Echo::NAME == "echo"`) whose associated functions have the declared signatures; documentation and visibility are kept. Each call sends `{"plugin":"echo","method":"say","args":{"text":"hi"}}`: the arguments travel as a JSON object **by name** (`quench-serde`), and the answer is read as the type inside `Result`. The Kotlin class with the same plugin name and the same method names is the other half; nothing checks that the two agree except running the app.

Rules, each a compile error that says so: the plugin name is lowercase letters, digits and `_`; every item is an `async fn` with no body, no `self`, no generics and named arguments (`(a, b): (u8, u8)` is refused); the return type is `Result<T, BridgeError>`.
