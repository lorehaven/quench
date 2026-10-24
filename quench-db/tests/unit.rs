// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/backends_in_memory_tests.rs"]
mod backends_in_memory_tests;
#[path = "unit/catalog_tests.rs"]
mod catalog_tests;
#[path = "unit/error_tests.rs"]
mod error_tests;
#[path = "unit/lib_tests.rs"]
mod lib_tests;
#[path = "unit/migrations_tests.rs"]
mod migrations_tests;
#[cfg(feature = "postgres")]
#[path = "unit/runner_tests.rs"]
mod runner_tests;

quench_test::main!();
