---
title: Configuration
parent: quench-test
grand_parent: Crates
nav_order: 6
---

## Command line of a test binary

Parsed by `Options::parse` from `std::env::args().skip(1)`. This is what `cargo test -- <args>` forwards.

| Argument | Effect |
|---|---|
| `FILTER...` (positional) | run only tests whose name contains one of them (equals one, with `--exact`) |
| `--exact` | match names exactly |
| `--skip FILTER`, `--skip=FILTER` | leave out tests whose name contains FILTER (exactly equals, with `--exact`); repeatable |
| `--ignored` | run only the ignored tests |
| `--include-ignored` | run the ignored tests as well |
| `--list` | print `name: test` for each selected test and `N tests, 0 benchmarks`, then exit 0 (no report files); with `--ignored` only the ignored ones |
| `--format terse`, `--format=terse` | with `--list`: no closing count line (what `cargo nextest` reads). Any other `--format` value is ignored. |
| `--test-threads N`, `--test-threads=N` | tests run at once; `0` or non-numeric is ignored (default: available parallelism, at most the number of tests) |
| `-q`, `--quiet` | one character per test (`.` passed, `F` failed, `i` ignored), 100 per line |
| `--report-dir DIR`, `--report-dir=DIR` | write the reports there |
| `-h`, `--help` | print usage and exit 0 |
| `--nocapture`, `--show-output`, `--color WHEN`, `--format FMT`, `--logfile F`, `-Z ...`, `--report-time`, `--ensure-time` and any other `-flag` | accepted and ignored (`--color`, `--logfile`, `-Z` also skip their value) |

Test names are `<module path>::<function>`, with the integration test's crate name as the first segment (for example `unit::adds`). Output follows the standard harness: `running N tests`, `test NAME ... ok|FAILED|ignored[, reason]`, a `failures:` section with `---- NAME stdout ----` and the message, `test result: ok. N passed; M failed; K ignored; 0 measured; F filtered out; finished in S.SSs`. Exit code 0, or 101 when a test failed. The output of a test (`println!`) is **not** captured: it is printed as it happens; `--nocapture` changes nothing.

## Environment variables

| Variable | Read by | Meaning |
|---|---|---|
| `QUENCH_TEST_REPORT` | `report::report_directory` | `off`, `0` or `false`: write no reports |
| `QUENCH_TEST_REPORT_DIR` | `report::report_directory` | directory for the reports (overrides the default and, when set, also under nextest) |
| `NEXTEST` | `report::report_directory` | when set (by `cargo nextest`) and `QUENCH_TEST_REPORT_DIR` is not, no reports are written: every test is a process of its own and would overwrite the report |
| `CARGO_PKG_NAME` | the runner | the first half of the report title (`<package> · <binary>`; `tests` when unset) |
| `CARGO_BIN_EXE_<name>` | `cmd::Command::cargo_bin` | path of a binary of the package under test |

Default report directory: `<profile directory>/quench-test` (for example `target/debug/quench-test`) when the test executable sits in a `deps` directory, else `target/quench-test` relative to the current directory. `--report-dir` beats the environment.

## Report file names

A run writes `<stem>.json` and `<stem>.html`, where the stem is the title with ` · ` replaced by `-` and any character other than letters, digits, `-` and `_` replaced by `-` (`quench-args-unit`), then rebuilds `index.html` and `summary.json` (title "All test results") from every `*.json` in the directory except `summary.json`.
