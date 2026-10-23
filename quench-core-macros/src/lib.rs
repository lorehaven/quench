//! Proc macros re-exported by `quench-core`. Use them through that crate.
//!
//! The macro entry points only convert between `proc_macro` and `proc_macro2`;
//! the expansion lives in plain functions so it can be unit-tested.

mod async_trait;
mod error_derive;

use proc_macro::TokenStream;

/// Lets a trait (and its impls) declare `async fn`s while staying usable as
/// `dyn Trait`: each becomes a method returning a boxed, `Send` future.
///
/// `#[async_trait(?Send)]` drops the `Send` requirement.
#[proc_macro_attribute]
pub fn async_trait(attr: TokenStream, item: TokenStream) -> TokenStream {
    async_trait::expand(attr.into(), item.into()).into()
}

/// Implements `Display` and `std::error::Error` (and `From` for `#[from]`
/// fields) from `#[error("..")]` attributes.
///
/// `#[error("text {field} {0}")]` formats with the variant's fields,
/// `#[error(transparent)]` forwards to the single inner error, `#[from]`
/// also generates the `From` impl and marks the source, and `#[source]` (or a
/// field named `source`) marks the source without `From`.
#[proc_macro_derive(Error, attributes(error, from, source))]
pub fn derive_error(input: TokenStream) -> TokenStream {
    error_derive::expand(input.into()).into()
}
