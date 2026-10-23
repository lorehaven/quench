//! Proc macros behind `quench-http`'s "discovery through annotations"
//! model:
//!
//! - `#[get]`/`#[post]`/`#[put]`/`#[delete]`/`#[patch]` turn a handler
//!   function into a discoverable route (registered at link time, no
//!   `.service(handler)` list to maintain).
//! - `#[injectable]` turns a constructor function into a discoverable
//!   component in the dependency-injection graph.
//!
//! Generated code only ever references `::quench_http::...` paths (the
//! crate re-exports `async_trait`, `http`, and the `submit!` registry macro for this
//! purpose), so a crate using these macros needs `quench-http` as its only
//! new dependency.

mod expand;

use proc_macro::TokenStream;

/// Discoverable `GET` route: `#[get("/users/{id}")]`.
///
/// Path patterns are literal
/// text plus `{name}` (one segment) and the multi-segment captures
/// `{name:.+}` / `{name:.*}`, for cases like the docker registry API where a captured
/// segment has to swallow further `/`s.
#[proc_macro_attribute]
pub fn get(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::route("GET", attr.into(), item.into()).into()
}

/// Discoverable `POST` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn post(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::route("POST", attr.into(), item.into()).into()
}

/// Discoverable `PUT` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn put(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::route("PUT", attr.into(), item.into()).into()
}

/// Discoverable `DELETE` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn delete(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::route("DELETE", attr.into(), item.into()).into()
}

/// Discoverable `PATCH` route. See [`macro@get`].
#[proc_macro_attribute]
pub fn patch(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::route("PATCH", attr.into(), item.into()).into()
}

/// Registers a constructor function as a component in the dependency graph.
///
/// Every parameter must be `Arc<Dep>` - the container resolves
/// `Dep` (another `#[injectable]`, or a value seeded via
/// `ContainerBuilder::provide`) and passes it in; the return type (bare
/// `T`, or `Result<T, E>` for a fallible constructor) is what other
/// components can depend on:
///
/// ```ignore
/// #[injectable]
/// async fn build_user_db(db: Arc<Db>) -> UserDb {
///     UserDb::init(db).await
/// }
/// ```
#[proc_macro_attribute]
pub fn injectable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::injectable(item.into()).into()
}
