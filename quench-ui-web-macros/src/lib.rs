//! `#[plugin]`: a typed client for a native plugin of the host app.
//!
//! See `quench-ui-web` (module `bridge`) for what the calls are and how they travel.

mod expand;

use proc_macro::TokenStream;

/// Turns a trait of `async fn` declarations into a client for the plugin of that name.
///
/// ```ignore
/// #[quench_ui_web::plugin("echo")]
/// pub trait Echo {
///     /// Returns the text it was given.
///     async fn say(text: String) -> Result<String, BridgeError>;
///     async fn device() -> Result<Device, BridgeError>;
/// }
///
/// let reply = Echo::say("hi".into()).await?;
/// ```
///
/// The trait becomes a unit struct `Echo` whose associated functions have the declared signatures.
/// Each call sends `{"plugin":"echo","method":"say","args":{"text":"hi"}}` to the host: the
/// arguments travel as a JSON object by name, the result is read back as the type inside `Result`.
/// The Kotlin class with the same plugin name and method names is the other half of the contract.
#[proc_macro_attribute]
pub fn plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::plugin(attr.into(), item.into()).into()
}
