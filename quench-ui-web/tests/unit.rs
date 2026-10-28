// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, unsafe_code)]

#[path = "unit/async_tests.rs"]
mod async_tests;
#[path = "unit/bridge_tests.rs"]
mod bridge_tests;
#[path = "unit/dom_tests.rs"]
mod dom_tests;
#[path = "unit/ffi_tests.rs"]
mod ffi_tests;
#[path = "unit/platform_tests.rs"]
mod platform_tests;
#[path = "unit/plugin_tests.rs"]
mod plugin_tests;
#[path = "unit/router_tests.rs"]
mod router_tests;
#[path = "unit/sse_tests.rs"]
mod sse_tests;

quench_test::main!();
