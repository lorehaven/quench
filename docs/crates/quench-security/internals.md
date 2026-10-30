---
title: Internals
parent: quench-security
grand_parent: Crates
nav_order: 4
---

## Design

* **Gate as tests.** Every stage is a `quench-test` case, so a failure carries a reason and the HTML/JSON report is the security report. The nested attack suites run under `--test-threads=1`.
* **Failing closed.** A missing tool is a failed case (`run` reports `could not start ...`), a scan that sees no files is an error (`the scan saw no Rust files under ...`), and the fuzz cases fail when a run produced no final statistics or fewer than 10,000 executions.
* **Own scanners tested first.** `tests/scanners.rs` feeds each scanner a planted violation so "clean" means something.

## Fuzz harness

`tests/fuzz.rs` runs `cargo +nightly fuzz run <target> fuzz/corpus/<target> fuzz/seeds/<target> -- -max_total_time=<s> -max_len=8192 -rss_limit_mb=2048` for eight targets: `request_head`, `chunked_decoder`, `jwt` (deep: 3x time), `response_head`, `ws_frame`, `uri`, `cookie`, `multipart_boundary`. A run fails when the process fails or the output contains `SUMMARY: libFuzzer` or `Test unit written to`. The fuzz crate is a separate workspace (`fuzz/`) so `libfuzzer-sys` stays out of the main dependency graph; a crash lands in `fuzz/artifacts/<target>/`.

## Known limits of the scanners

* `line_uses_unsafe` is a line-pattern scan, not a parser: `unsafe(link_section = ...)` inside a `#[cfg_attr(...)]` line (as in `quench-core`'s `submit!` macro, `quench-core/src/registry.rs`) is not matched, so that file is not in `scripts/unsafe-allowlist.txt` even though its expansion contains the keyword.
* The secrets scan looks for four token shapes only; it skips `.md` files and anything under `tests`, `testdata`, `fixtures`.
