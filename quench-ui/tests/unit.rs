// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::wildcard_imports,
    clippy::unreadable_literal,
    clippy::ignored_unit_patterns,
    clippy::similar_names,
    clippy::redundant_clone,
    clippy::semicolon_if_nothing_returned,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::too_many_lines,
    clippy::missing_const_for_fn,
    clippy::format_collect,
    clippy::redundant_closure_for_method_calls
)]

#[path = "unit/reactive_tests.rs"]
mod reactive_tests;

#[path = "unit/html_tests.rs"]
mod html_tests;
#[path = "unit/keyed_tests.rs"]
mod keyed_tests;
#[path = "unit/view_tests.rs"]
mod view_tests;

quench_test::main!();
