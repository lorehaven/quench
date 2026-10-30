---
title: Configuration
parent: quench-security
grand_parent: Crates
nav_order: 3
---

| Variable | Read by | Default | Meaning |
|---|---|---|---|
| `QUENCH_TEST_REPORT_DIR` | the `quench-test` harness (and the nested runs that inherit it) | unset | directory for the JSON/HTML report (`summary.json`, `index.html`) |
| `QUENCH_FUZZ_SECONDS` | `tests/fuzz.rs` | `60` | base fuzzing time per target; `request_head`, `chunked_decoder` and `jwt` get three times as much. An unparseable value gives the default |
| `QUENCH_FUZZ_ITERS` | `tests/gate.rs` `attack_parser_properties` (passed on to `fuzz_parsers`) | `200000` | iterations of the mutation-fuzzed parser properties; `security.fish --deep` sets `1000000` |

Files read by the gate (relative to the workspace root = the parent of the `quench-security` directory):

| File | Role |
|---|---|
| `scripts/unsafe-allowlist.txt` | required; one path (`./crate/src/file.rs`) per line, `#` comments and blank lines ignored |
| `scripts/secrets-allowlist.txt` | optional; paths exempt from the secrets scan |
| `deny.toml` | read by `cargo deny` (see [Dependency policy](../../guides/dependency-policy.md)) |
| `.cargo/audit.toml` | read by `cargo audit` (`[advisories] ignore = []`) |
| `fuzz/seeds/<target>`, `fuzz/corpus/<target>` | seeds are fixed; the corpus directory is created and grows (the corpus is rebuilt from the seeds each run unless it is restored from an earlier run) |

libFuzzer is run with `-max_total_time=<seconds> -max_len=8192 -rss_limit_mb=2048`.
