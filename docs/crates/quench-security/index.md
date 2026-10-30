---
title: quench-security
parent: Crates
nav_order: 41
has_children: true
permalink: /crates/quench-security/
---

`quench-security` (library `quench_security`) is the security gate of the workspace written as a test crate: supply-chain checks, static scans and attack suites run as `quench-test` cases, so the security report is an ordinary `quench-test` report (JSON and HTML) that the CI shows next to the unit tests. The library itself contains the scanners and command helpers; the stages are the tests under `tests/`.

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 (workspace) |
| `publish` | `false` (never released) |
| Feature | `gate` (off by default) |
| Dependencies | none; dev-dependencies `quench-test`, `quench-core` |
| Test harness | `harness = false` for the three integration tests (the library has none) |

## Feature flag

| Feature | Default | Effect |
|---|---|---|
| `gate` | off | enables the integration tests `gate` and `fuzz` (`required-features = ["gate"]`). They shell out to `cargo-deny`, `cargo-audit` and nested `cargo test` runs, read the network (advisory database) and, for `fuzz`, run for minutes. Off by default so `cargo test --workspace` never starts them |

The `scanners` test needs no feature and is always on.

## Stages (what the gate checks)

| Test | Stage | Needs |
|---|---|---|
| `tests/scanners.rs` | the scanners' own tests: each scanner is shown known-bad input | nothing |
| `tests/gate.rs` `supply_chain_cargo_deny` | `cargo deny check` (licenses, bans: the dependency allowlist in `deny.toml`, sources, advisories) | `cargo-deny` |
| `tests/gate.rs` `supply_chain_cargo_audit` | `cargo audit --deny warnings` against `Cargo.lock` | `cargo-audit`, network |
| `static_unsafe_is_confined_to_allowlisted_files` | files using `unsafe` must be in `scripts/unsafe-allowlist.txt`; stale entries fail too | git |
| `static_no_secrets_in_sources` | key material and cloud tokens in project text files | git |
| `static_no_symlink_points_outside_the_checkout` | no tracked symlink to an absolute or escaping target | git |
| `attack_http_smuggling_and_resource_exhaustion` | nested `cargo test -p quench-proto --features server,client --test security_h1` | |
| `attack_parser_properties` | nested `cargo test -p quench-proto --features server,client --test fuzz_parsers` with `QUENCH_FUZZ_ITERS` (default 200000) | |
| `attack_jwt_verification` | nested `cargo test -p quench-crypto --test security_jwt` | |
| `attack_framework_layer` | nested `cargo test -p quench-http --test security_http` | |
| `tests/fuzz.rs` (8 cases) | coverage-guided fuzzing with `cargo +nightly fuzz run` per target | `cargo-fuzz`, nightly |

See [Usage](usage.md) for the commands, [Configuration](configuration.md), [API](api.md), [Internals](internals.md), [Testing](testing.md). The wider policy is in the guide [Dependency policy](../../guides/dependency-policy.md).
