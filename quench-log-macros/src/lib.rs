//! `#[instrument]` for `quench-log`. Use it through that crate.

mod instrument;

use proc_macro::TokenStream;

/// Runs a function inside a span named after it, with its arguments recorded
/// as fields (`Debug`).
///
/// ```ignore
/// #[quench_log::instrument]
/// async fn publish(name: String, version: u32) { .. }
///
/// #[quench_log::instrument(skip(db, body), fields(size = body.len()), level = "debug")]
/// async fn upload(db: &Db, body: Vec<u8>) { .. }
/// ```
///
/// Options: `skip(a, b)` leaves those arguments out, `skip_all` leaves out all
/// of them, `fields(name = expr, other = %expr)` adds fields (same syntax as
/// the `span!` macro), `level = "debug"` sets the span's level (default
/// `info`), `name = "other"` renames the span. An `async fn` is instrumented
/// as a future, so the span is entered on every poll.
#[proc_macro_attribute]
pub fn instrument(attr: TokenStream, item: TokenStream) -> TokenStream {
    instrument::expand(attr.into(), item.into()).into()
}
