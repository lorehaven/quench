---
title: Usage
parent: quench-security
grand_parent: Crates
nav_order: 1
---

## Run the whole gate

```bash
scripts/security.fish             # scanners, then the gate (supply chain, static, attack suites)
scripts/security.fish --deep      # QUENCH_FUZZ_ITERS=1000000 for the parser property tests (minutes)
scripts/security.fish --fuzz      # then coverage-guided fuzzing, 60 s per target (3x for the deep ones)
```

The script sets `QUENCH_TEST_REPORT_DIR=$PWD/target/security`, deletes that directory first, runs

```bash
cargo test -p quench-security --test scanners
cargo test -p quench-security --features gate --test gate -- --test-threads=1
cargo test -p quench-security --features gate --test fuzz -- --test-threads=1     # with --fuzz only
```

and prints `report: target/security/index.html`; exit status is non-zero if any stage fails. `--test-threads=1` is required: the attack-suite cases are nested `cargo test` runs. Each nested run inherits `QUENCH_TEST_REPORT_DIR`, so its tests appear in the same report.

## Run only the fast stages by hand

```bash
QUENCH_TEST_REPORT_DIR=$PWD/target/security \
    cargo test -p quench-security --features gate --test gate -- --test-threads=1
```

## Fuzz with a longer budget

```bash
QUENCH_FUZZ_SECONDS=300 QUENCH_TEST_REPORT_DIR=$PWD/target/security \
    cargo test -p quench-security --features gate --test fuzz -- --test-threads=1
```

## Use the scanners in your own test

```rust
use quench_security::{files_using_unsafe, secrets_in, workspace_root};

let root = workspace_root();
for file in files_using_unsafe(&root) { println!("{file}"); }   // "./path.rs"
for (file, line, what) in secrets_in(&root) { eprintln!("{file}:{line} {what}"); }
```

Allowing a reviewed unsafe file means adding a line to `scripts/unsafe-allowlist.txt`; a reviewed secrets false positive goes in `scripts/secrets-allowlist.txt` (the gate reads it if it exists; it does not exist in the repository today).

## CI

`.conveyor.toml` runs the gate nightly (schedule `0 0 * * *`, timezone `Europe/Warsaw`) as the `security` stage, one job with `QUENCH_FUZZ_SECONDS=120` and `QUENCH_FUZZ_ITERS=1000000` and a timeout of 5400 s. Each step is chained with `|| rc=1` so a failed stage does not stop the fuzzing after it.
