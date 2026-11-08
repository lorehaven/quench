// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::missing_const_for_fn,
    clippy::literal_string_with_formatting_args,
    // Snapshot hashes are printed and pasted whole; geometry reads as the specification writes it.
    clippy::unreadable_literal,
    clippy::suboptimal_flops
)]

#[path = "typeset/fuzz_tests.rs"]
mod fuzz_tests;
#[path = "typeset/geometry_tests.rs"]
mod geometry_tests;
#[path = "typeset/snapshot_tests.rs"]
mod snapshot_tests;

quench_test::main!();
