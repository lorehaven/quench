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

#[path = "unit/anim_tests.rs"]
mod anim_tests;
#[path = "unit/reactive_tests.rs"]
mod reactive_tests;

#[path = "unit/form_tests.rs"]
mod form_tests;
#[path = "unit/gesture_tests.rs"]
mod gesture_tests;
#[path = "unit/html_tests.rs"]
mod html_tests;
#[path = "unit/i18n_tests.rs"]
mod i18n_tests;
#[path = "unit/keyed_tests.rs"]
mod keyed_tests;
#[path = "unit/kit_tests.rs"]
mod kit_tests;
#[path = "unit/router_tests.rs"]
mod router_tests;
#[path = "unit/view_tests.rs"]
mod view_tests;
#[path = "unit/virtual_list_tests.rs"]
mod virtual_list_tests;

quench_test::main!();
