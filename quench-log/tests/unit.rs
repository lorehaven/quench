// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/config_tests.rs"]
mod config_tests;
#[path = "unit/init_tests.rs"]
mod init_tests;
#[path = "unit/instrument_tests.rs"]
mod instrument_tests;
#[path = "unit/macros_tests.rs"]
mod macros_tests;

quench_test::main!();
