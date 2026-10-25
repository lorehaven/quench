//! Coverage-guided fuzzing (libFuzzer via cargo-fuzz), one case per target.
//!
//! `QUENCH_FUZZ_SECONDS` is the base time per target (default 60); the targets with real parsing
//! state get three times that. A crash fails the case and names the reproducer. The corpus is rebuilt
//! from `fuzz/seeds` each run unless `fuzz/corpus` is restored from an earlier one.
//!
//! Needs `--features gate`, `cargo install cargo-fuzz` and a nightly toolchain: a missing tool is a
//! failed case, never a skipped one.

#![allow(clippy::wildcard_imports)]

use quench_security::*;

fn seconds(deep: bool) -> u64 {
    let base: u64 = std::env::var("QUENCH_FUZZ_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    if deep { base * 3 } else { base }
}

fn fuzz(target: &str, deep: bool) {
    let root = workspace_root();
    let corpus = format!("fuzz/corpus/{target}");
    std::fs::create_dir_all(root.join(&corpus)).expect("corpus directory");
    let seeds = format!("fuzz/seeds/{target}");
    let limit = format!("-max_total_time={}", seconds(deep));

    let ran = run(
        &root,
        "cargo",
        &[
            "+nightly",
            "fuzz",
            "run",
            target,
            &corpus,
            &seeds,
            "--",
            &limit,
            "-max_len=8192",
            "-rss_limit_mb=2048",
        ],
        &[],
    );
    let crash =
        ran.output.contains("SUMMARY: libFuzzer") || ran.output.contains("Test unit written to");
    assert!(
        ran.success && !crash,
        "{target}: libFuzzer found a crash or the run failed.\n{}",
        ran.tail(40)
    );
    let stats = fuzz_stats(&ran.output).unwrap_or_else(|| {
        panic!(
            "{target}: no final statistics - the run did not complete:\n{}",
            ran.tail(20)
        )
    });
    // A fuzzer that executed almost nothing proves nothing.
    assert!(
        stats.execs > 10_000,
        "{target}: only {} executions in {}s",
        stats.execs,
        seconds(deep)
    );
    println!(
        "{target}: {} execs, {} edges, {} features",
        stats.execs, stats.coverage, stats.features
    );
}

#[quench_test::test]
fn fuzz_request_head() {
    fuzz("request_head", true);
}
#[quench_test::test]
fn fuzz_chunked_decoder() {
    fuzz("chunked_decoder", true);
}
#[quench_test::test]
fn fuzz_jwt() {
    fuzz("jwt", true);
}
#[quench_test::test]
fn fuzz_response_head() {
    fuzz("response_head", false);
}
#[quench_test::test]
fn fuzz_ws_frame() {
    fuzz("ws_frame", false);
}
#[quench_test::test]
fn fuzz_uri() {
    fuzz("uri", false);
}
#[quench_test::test]
fn fuzz_cookie() {
    fuzz("cookie", false);
}
#[quench_test::test]
fn fuzz_multipart_boundary() {
    fuzz("multipart_boundary", false);
}

quench_test::main!();
