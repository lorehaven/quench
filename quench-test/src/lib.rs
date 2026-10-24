//! A light test runtime with JSON and HTML result reports, and what tests commonly need around it.
//!
//! - The runtime: `#[quench_test::test]` replaces `#[test]` and `#[tokio::test]`; a test target sets
//!   `harness = false` in `Cargo.toml` and calls [`main!`]. It accepts the command line `cargo test` passes,
//!   prints what the standard harness prints, and writes `<name>.json` and `<name>.html` plus a merged
//!   `index.html` / `summary.json` for every test binary run so far (see [`report`]).
//! - [`cmd`]: run a binary and assert on its exit status and output.
//! - [`predicate`]: the string predicates those assertions use.
//! - [`mock`]: an HTTP server that answers from rules and records what it was asked.
//!
//! ```ignore
//! // tests/unit.rs, with `[[test]] name = "unit" harness = false` in Cargo.toml
//! use quench_test::test;
//!
//! #[test]
//! fn adds() {
//!     assert_eq!(2 + 2, 4);
//! }
//!
//! #[test(flavor = "multi_thread", worker_threads = 2)]
//! async fn fetches() -> Result<(), String> {
//!     Ok(())
//! }
//!
//! quench_test::main!();
//! ```

// The mock server's methods are `async` to match the shape tests are written against; report figures
// convert counts to floats for display.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::option_if_let_else,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::unused_async_trait_impl,
    clippy::must_use_candidate,
    clippy::return_self_not_must_use,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::format_push_string,
    clippy::missing_fields_in_debug,
    clippy::needless_pass_by_value,
    clippy::significant_drop_tightening,
    clippy::unused_async
)]

mod case;
pub mod cmd;
pub mod mock;
pub mod predicate;
pub mod report;
mod runner;

#[doc(hidden)]
pub use case::private as __private;
pub use case::{
    Outcome, ShouldPanic, TESTS, Termination, TestCase, panic_message, silence_panics,
    take_panic_location,
};
#[doc(hidden)]
pub use quench_core as __core;
pub use quench_test_macros::test;
pub use runner::{Options, RunResult, run, run_registered};

/// The entry point of a test binary with `harness = false`: runs every `#[test]` and exits with the
/// standard harness's status.
#[macro_export]
macro_rules! main {
    () => {
        fn main() {
            ::std::process::exit($crate::run_registered());
        }
    };
}
