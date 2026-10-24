// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

#[path = "unit/address_tests.rs"]
mod address_tests;
#[path = "unit/message_tests.rs"]
mod message_tests;
#[path = "unit/mime_tests.rs"]
mod mime_tests;
#[path = "unit/smtp_tests.rs"]
mod smtp_tests;
#[path = "unit/support/mod.rs"]
mod support;

quench_test::main!();
