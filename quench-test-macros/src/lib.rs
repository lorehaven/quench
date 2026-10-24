//! The `#[test]` attribute of `quench-test`. Use it through that crate.
//!
//! The entry point only converts between `proc_macro` and `proc_macro2`; the expansion is a plain function
//! so it can be unit-tested.

mod expand;

use proc_macro::TokenStream;

/// Marks a function as a test of the `quench-test` runner (a replacement for `#[test]` and
/// `#[tokio::test]`).
///
/// - A plain `fn` runs on a test thread. An `async fn` runs on a Tokio runtime built for it:
///   current-thread by default, or `#[test(flavor = "multi_thread", worker_threads = 2)]`.
/// - The function may return `()` or `Result<(), E>` where `E: Debug`; an `Err` fails the test.
/// - `#[ignore]` / `#[ignore = "why"]` and `#[should_panic]` / `#[should_panic(expected = "text")]` work as
///   with the standard harness.
///
/// The test binary needs `harness = false` and a `quench_test::main!();`.
#[proc_macro_attribute]
pub fn test(attr: TokenStream, item: TokenStream) -> TokenStream {
    expand::expand(attr.into(), item.into()).into()
}
