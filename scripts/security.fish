#!/usr/bin/env fish
# Security gate for quench. The gate itself is the `quench-security` test crate - one case per stage, so
# its report is a quench-test report (the same one conveyor's nightly job keeps and shows).
#
#   scripts/security.fish            supply chain, static scans, attack suites
#   scripts/security.fish --deep     ... with 1M fuzz iterations per parser property (minutes)
#   scripts/security.fish --fuzz     ... then coverage-guided fuzzing, 60s per target (3x for the deep ones)
#
# The report lands in target/security/index.html. Exit status is non-zero if any stage fails.

cd (dirname (status filename))/..

set -lx QUENCH_TEST_REPORT_DIR $PWD/target/security
rm -rf $QUENCH_TEST_REPORT_DIR
contains -- --deep $argv; and set -lx QUENCH_FUZZ_ITERS 1000000

set -l failed 0
cargo test -p quench-security --test scanners; or set failed 1
cargo test -p quench-security --features gate --test gate -- --test-threads=1; or set failed 1
if contains -- --fuzz $argv
    cargo test -p quench-security --features gate --test fuzz -- --test-threads=1; or set failed 1
end

echo
echo "report: $QUENCH_TEST_REPORT_DIR/index.html"
test $failed -eq 0; and echo "security gate passed"; or begin
    echo "security gate FAILED"
    exit 1
end
