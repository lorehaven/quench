# quench-security

The security gate as a test run, so its report is a `quench-test` report: every stage is a case, every
failure carries the reason, and conveyor shows it on the repository's scan page like the unit tests.

    # everything fast (supply chain, static scans, attack suites)
    QUENCH_TEST_REPORT_DIR=$PWD/target/security \
        cargo test -p quench-security --features gate --test gate -- --test-threads=1

    # coverage-guided fuzzing, QUENCH_FUZZ_SECONDS per target (default 60; the deeper targets get 3x)
    QUENCH_FUZZ_SECONDS=300 QUENCH_TEST_REPORT_DIR=$PWD/target/security \
        cargo test -p quench-security --features gate --test fuzz -- --test-threads=1

`target/security/summary.json` + `index.html` are what the nightly pipeline keeps. The scanners themselves
(`src/lib.rs`) are plain functions with their own always-on tests (`tests/scanners.rs`).

## Documentation

Full documentation: https://lorehaven.github.io/quench/crates/quench-security/
