// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::wildcard_imports,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::float_cmp, // fixed-point values are exact in binary
    clippy::too_many_lines
)]
#![cfg(target_os = "linux")]

#[path = "unit/client_tests.rs"]
mod client_tests;
#[path = "unit/compositor_tests.rs"]
mod compositor_tests;
#[path = "unit/keymap_tests.rs"]
mod keymap_tests;
#[path = "unit/sys_tests.rs"]
mod sys_tests;
#[path = "unit/window_tests.rs"]
mod window_tests;
#[path = "unit/wire_tests.rs"]
mod wire_tests;

quench_test::main!();
