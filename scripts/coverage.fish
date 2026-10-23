#!/usr/bin/env fish
# Per-crate line coverage with a gate.
#
#   scripts/coverage.fish                  every workspace library crate
#   scripts/coverage.fish quench-http ...  only the named crates
#
# Environment:
#   MIN_COVERAGE   threshold in percent (default 90)
#   NO_FAIL=1      print the table but always exit 0 (baseline runs)
#
# If a crate was restructured heavily, stale test binaries in target/llvm-cov-target
# are still picked up and show up as phantom uncovered lines (line numbers past
# the end of a file are the tell). `cargo llvm-cov clean --workspace` fixes it.
#
# stdin is closed for the test runs: quench-cli's tests read it (rustyline) and
# would hang forever on an open pipe.
#
# Test files, examples and the proc-macro shims' generated code are not counted:
# the gate is about the library sources.

set -l min 90
set -q MIN_COVERAGE; and set min $MIN_COVERAGE

cd (dirname (status filename))/..

set -l crates $argv
if test (count $crates) -eq 0
    set crates (cargo metadata --format-version 1 --no-deps | jq -r \
        '.workspace_members as $m | .packages[] | select(.id as $i | $m | index($i)) | select(.source == null) | select(.manifest_path | contains("/examples/") | not) | .name')
end

set -l ignore '(/tests/|/examples/|/target/)'
set -l failed
printf '%-26s %9s %9s %9s\n' crate lines% funcs% regions%

for crate in $crates
    set -l json (cargo llvm-cov --package $crate --all-features --json --summary-only \
        --ignore-filename-regex $ignore 2>/dev/null </dev/null)
    if test $status -ne 0; or test -z "$json"
        printf '%-26s %9s\n' $crate 'TESTS FAILED'
        set -a failed $crate
        continue
    end
    set -l lines (echo $json | jq -r '.data[0].totals.lines.percent')
    set -l funcs (echo $json | jq -r '.data[0].totals.functions.percent')
    set -l regions (echo $json | jq -r '.data[0].totals.regions.percent')
    printf '%-26s %9.2f %9.2f %9.2f\n' $crate $lines $funcs $regions
    if awk -v l=$lines -v m=$min 'BEGIN { exit !(l < m) }'
        set -a failed $crate
    end
end

if test (count $failed) -gt 0
    echo
    echo "below $min% (or failing): $failed"
    set -q NO_FAIL; and exit 0
    exit 1
end
