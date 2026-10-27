// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports, unsafe_code)]

#[path = "unit/async_tests.rs"]
mod async_tests;
#[path = "unit/dom_tests.rs"]
mod dom_tests;
#[path = "unit/ffi_tests.rs"]
mod ffi_tests;

quench_test::main!();
