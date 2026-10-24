// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[cfg(feature = "repl")]
#[path = "unit/repl_tests.rs"]
mod repl_tests;
#[path = "unit/require_tests.rs"]
mod require_tests;
#[path = "unit/terminal_tests.rs"]
mod terminal_tests;

quench_test::main!();
