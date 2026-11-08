// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else,
    // TeX's braces look like format arguments to clippy.
    clippy::literal_string_with_formatting_args
)]

#[path = "unit/block_tests.rs"]
mod block_tests;
#[path = "unit/error_tests.rs"]
mod error_tests;
#[path = "unit/fuzz_tests.rs"]
mod fuzz_tests;
#[path = "unit/inline_tests.rs"]
mod inline_tests;
#[path = "unit/macro_tests.rs"]
mod macro_tests;
#[path = "unit/regression_tests.rs"]
mod regression_tests;
#[path = "unit/tree_tests.rs"]
mod tree_tests;

quench_test::main!();
