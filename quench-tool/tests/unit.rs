// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::needless_pass_by_value,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::option_if_let_else,
    clippy::unnecessary_literal_unwrap,
    clippy::redundant_closure_for_method_calls,
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_ref_mut,
    clippy::format_collect,
    clippy::wildcard_imports,
    clippy::map_unwrap_or,
    clippy::unnecessary_debug_formatting,
    unused_imports
)]

#[path = "unit/support.rs"]
mod support;

#[path = "unit/browser_tests.rs"]
mod browser_tests;
#[path = "unit/cli_tests.rs"]
mod cli_tests;
#[path = "unit/dev_tests.rs"]
mod dev_tests;
#[path = "unit/doctor_tests.rs"]
mod doctor_tests;
#[path = "unit/project_tests.rs"]
mod project_tests;
#[path = "unit/release_tests.rs"]
mod release_tests;
#[path = "unit/scaffold_tests.rs"]
mod scaffold_tests;
#[path = "unit/system_tests.rs"]
mod system_tests;
#[path = "unit/watch_tests.rs"]
mod watch_tests;

quench_test::main!();
