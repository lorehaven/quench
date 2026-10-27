// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::needless_pass_by_value,
    clippy::future_not_send,
    clippy::format_push_string,
    clippy::derive_partial_eq_without_eq,
    unused_imports,
    clippy::unused_async,
    clippy::items_after_statements,
    clippy::wildcard_imports,
    clippy::unreadable_literal,
    clippy::cast_possible_truncation,
    clippy::redundant_clone,
    clippy::redundant_closure_for_method_calls
)]

#[path = "unit/assets_tests.rs"]
mod assets_tests;
#[path = "unit/dev_tests.rs"]
mod dev_tests;
#[path = "unit/document_tests.rs"]
mod document_tests;
#[path = "unit/support.rs"]
mod support;

#[path = "unit/error_tests.rs"]
mod error_tests;
#[path = "unit/function_tests.rs"]
mod function_tests;
#[path = "unit/functions.rs"]
mod functions;
#[path = "unit/guard_tests.rs"]
mod guard_tests;
#[path = "unit/routing_tests.rs"]
mod routing_tests;
#[path = "unit/session_tests.rs"]
mod session_tests;
#[path = "unit/stub_tests.rs"]
mod stub_tests;

quench_test::main!();
