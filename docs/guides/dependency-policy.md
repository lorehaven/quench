---
title: Dependency policy
parent: Guides
nav_order: 3
---

Quench keeps its third-party dependency set deliberately small and enforces it mechanically. This page describes the policy, the files that encode it and the checks that run.

## The rule

Everything outside the workspace must be one of a short list of crates. The root `deny.toml` states the target: `libc`, `getrandom`, `tokio` (and its own support crates), `rustls`, `ring`, `webpki-roots`, and `proc-macro2`, `syn`, `quote` (derive macros only). The workspace manifest pins exactly these as the only third-party entries in `[workspace.dependencies]`:

| Crate | Version / features (root `Cargo.toml`) | Used by |
|---|---|---|
| `getrandom` | `0.4` | `quench-core` (non-wasm) |
| `libc` | `0.2` | `quench-desktop-wayland` |
| `proc-macro2`, `quote` | `1.0` | every `-macros` crate |
| `syn` | `3.0`, feature `full` | every `-macros` crate (`quench-core-macros` adds `visit-mut`) |
| `ring` | `0.17`, `default-features = false`, `["alloc", "std"]` | `quench-crypto` |
| `rustls` | `0.23`, `default-features = false`, `["ring", "std", "tls12", "logging"]` | `quench-http`, `quench-tls` |
| `tokio` | `1.53`, features `rt-multi-thread macros net io-util signal sync time` | async crates |
| `webpki-roots` | `1.0` | `quench-tls` |

`cargo metadata --all-features` currently reports exactly these nine as direct third-party dependencies of workspace crates, and 41 distinct third-party crates in total (the nine plus their support crates). Quench crates replace the usual ecosystem crates: for example `quench-core` stands in for `chrono`, `uuid`, `anyhow`, `semver`, `bytes`, `walkdir` and others; `quench-serde` for `serde`, `serde_json`, `toml` and the YAML crate; `quench-log` for `tracing`.

## `deny.toml`

Checked by `cargo deny check` (the file header is the default template, edited):

* **`[graph]`**: `all-features = true`, `no-default-features = false`.
* **`[bans]`**: `multiple-versions = "warn"`, `wildcards = "allow"`, `workspace-default-features` and `external-default-features` `"allow"`, `deny = []`, `skip`/`skip-tree` empty. The **allowlist**: `allow = [ ... ]` names 41 crates and `allow-workspace = true` lets every workspace crate through. Anything not listed fails `cargo deny check bans`, so a new third-party crate cannot slip in. The list is the nine crates above plus: `bytes`, `cc`, `cfg-if`, `errno`, `find-msvc-tools`, `log`, `mio`, `once_cell`, `pin-project-lite`, `r-efi`, `rustls-pki-types`, `rustls-webpki`, `shlex`, `signal-hook-registry`, `socket2`, `subtle`, `tokio-macros`, `unicode-ident`, `untrusted`, `wasi`, `windows-link`, `windows-sys`, `windows-targets`, the eight `windows_*` target crates, and `zeroize`. These are pulled in by `tokio`, `ring`, `rustls`, `webpki-roots` and `getrandom` (checked with the resolved graph). The file's own comment says each cleanup task should delete entries it makes unnecessary; `cargo deny` warns about allowed crates that no longer appear.
* **`[licenses]`**: allowed SPDX identifiers `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-1-Clause`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Zlib`, `0BSD`, `BSL-1.0`, `Unicode-3.0`, `Unlicense`, `CC0-1.0`, `MIT-0`, `CDLA-Permissive-2.0`, `LGPL-2.1-or-later` (only ever one arm of an `OR` expression); `confidence-threshold = 0.8`; private (unpublished or private-registry) workspace crates are ignored (`[licenses.private] ignore = true`).
* **`[advisories]`**: `ignore = []` (and the same in `.cargo/audit.toml`).
* **`[sources]`**: `unknown-registry = "warn"`, `unknown-git = "warn"`, `allow-registry = ["https://github.com/rust-lang/crates.io-index"]`, no allowed git organisations.

## Tools in `scripts/`

| Script | Purpose |
|---|---|
| `scripts/deps.fish` | prints `third-party crates: N` (unique names across all targets and features, the same graph `cargo deny check bans` enforces); `-v` lists the names |
| `scripts/prune-deny.py` | rewrites the `[bans] allow` list in `deny.toml` to exactly the crates now in the graph, so the list only shrinks as dependencies are removed (it keeps `allow-workspace` and the rest of the file) |
| `scripts/features.fish` | builds and lints every crate that has features with defaults off and with each feature alone (a feature that only compiles next to others is caught) |
| `scripts/security.fish` | the security gate, below |
| `scripts/unsafe-allowlist.txt` | files allowed to contain `unsafe` |

## What runs, and where

The pipeline in `.conveyor.toml` has, on every push to `master` and every pull request, a `check` stage with three jobs: `format` (`anvil format --check`), `lint` (`anvil lint --all-targets --deny-warnings`) and `deps` (`anvil machete`, `anvil audit`, `anvil deny`). `anvil` is the workspace's build tool; this repository does not contain its source, so the mapping of those three subcommands to `cargo machete` (unused dependencies), `cargo audit` and `cargo deny` is *unverified* here. Unused dependencies are what `cargo-machete` finds: a dependency declared in `Cargo.toml` and never used in the source. After `check` come `build` (`anvil build --all`) and `test` (`anvil test --all` with a coverage job using `cargo llvm-cov`). A separate nightly `security` stage (schedule `0 0 * * *`, Europe/Warsaw) runs the security gate: advisories can appear without any code changing.

## The security gate

The gate is the [quench-security](../crates/quench-security/index.md) test crate; `scripts/security.fish` runs it. Its stages:

1. **Supply chain**: `cargo deny check` and `cargo audit --deny warnings`.
2. **Static**: files that use `unsafe` must be listed in `scripts/unsafe-allowlist.txt` (and stale entries fail); no private keys, AWS key ids, GitHub tokens or Slack tokens in project text files; no tracked symlink pointing outside the checkout.
3. **Attack suites**: wire-level HTTP attacks (`quench-proto`, `security_h1`), mutation-fuzzed parser properties (`fuzz_parsers`), JWT attacks (`quench-crypto`, `security_jwt`), framework-layer attacks (`quench-http`, `security_http`).
4. **Fuzzing** (`--fuzz`): libFuzzer targets in `fuzz/` for the request head, response head, chunked decoder, WebSocket frames, URIs, cookies, multipart boundaries and JWT.

## `unsafe`

The workspace lint is `unsafe_code = "deny"`. `scripts/unsafe-allowlist.txt` currently lists the reviewed exceptions: `quench-core/src/env.rs` (`set_var`/`remove_var`), `quench-ui/src/lib.rs` (names the lint switch), `quench-ui-web/src/ffi.rs` and `host/wasm.rs` (the wasm boundary), `quench-desktop/src/ffi.rs` (GTK) and `quench-desktop-wayland/src/sys.rs` (sockets and shared memory). The scanner is line-pattern based; see the limits in [quench-security internals](../crates/quench-security/internals.md).

## Adding or removing a dependency

1. Add a third-party crate only if it is within the allowed set; otherwise the change needs the `deny.toml` allowlist, which is a deliberate policy decision.
2. Put it in `[workspace.dependencies]` and refer to it with `{ workspace = true }`.
3. After removing a dependency, run `scripts/prune-deny.py` and review the shrunk list.
