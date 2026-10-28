// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::wildcard_imports,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

#[path = "unit/serve_tests.rs"]
mod serve_tests;
#[path = "unit/tz_tests.rs"]
mod tz_tests;

quench_test::main!();
