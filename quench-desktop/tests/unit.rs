// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, clippy::unwrap_used)]
#![cfg(target_os = "linux")]

#[path = "unit/bridge_tests.rs"]
mod bridge_tests;
#[path = "unit/bundle_tests.rs"]
mod bundle_tests;
#[path = "unit/host_tests.rs"]
mod host_tests;
#[path = "unit/plugin_tests.rs"]
mod plugin_tests;
#[path = "unit/window_tests.rs"]
mod window_tests;

quench_test::main!();
