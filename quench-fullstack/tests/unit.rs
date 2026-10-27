// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::wildcard_imports,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls
)]

#[path = "unit/assets_tests.rs"]
mod assets_tests;
#[path = "unit/document_tests.rs"]
mod document_tests;
#[path = "unit/support.rs"]
mod support;

quench_test::main!();
