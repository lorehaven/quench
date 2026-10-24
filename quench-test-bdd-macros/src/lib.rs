//! `#[given]`, `#[when]`, `#[then]` and `#[derive(World)]` for `quench-test-bdd`. Use them through that crate.
//!
//! The entry points only convert between `proc_macro` and `proc_macro2`; the expansion lives in plain
//! functions so it can be unit-tested.

mod steps;
mod world;

use proc_macro::TokenStream;

/// Registers a function as a `Given` step.
///
/// ```ignore
/// #[given("a user {string} with {int} points")]
/// async fn user(world: &mut MyWorld, name: String, points: u32) { /* ... */ }
///
/// #[given(regex = r"^there are (\d+) users$")]
/// fn users(world: &mut MyWorld, count: usize) { /* ... */ }
/// ```
///
/// The first parameter is `&mut World`. The others take the captures in order (any `FromStr` type), except
/// one of type `&Step`, which receives the step itself (its doc string and table). The function may be
/// `async`, and may return `()` or `Result<(), E>` with `E: Display`.
#[proc_macro_attribute]
pub fn given(attr: TokenStream, item: TokenStream) -> TokenStream {
    steps::expand("Given", attr.into(), item.into()).into()
}

/// Registers a function as a `When` step; see [`given`].
#[proc_macro_attribute]
pub fn when(attr: TokenStream, item: TokenStream) -> TokenStream {
    steps::expand("When", attr.into(), item.into()).into()
}

/// Registers a function as a `Then` step; see [`given`].
#[proc_macro_attribute]
pub fn then(attr: TokenStream, item: TokenStream) -> TokenStream {
    steps::expand("Then", attr.into(), item.into()).into()
}

/// Implements `World` for a type: each scenario gets a fresh value.
///
/// `#[world(init = path)]` names an `async fn() -> Self` that builds it; without the attribute the type
/// must implement `Default`.
#[proc_macro_derive(World, attributes(world))]
pub fn derive_world(input: TokenStream) -> TokenStream {
    world::expand(input.into()).into()
}
