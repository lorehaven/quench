---
title: API
parent: quench-security
grand_parent: Crates
nav_order: 2
---

All public items of `quench_security` (`src/lib.rs`). They are plain functions over text and the filesystem so each can be tested against known-bad input. The patterns are assembled from pieces at run time so the file does not trip its own scan.

## File discovery

Files are those git considers part of the project: `git ls-files --cached --others --exclude-standard`, restricted to regular files (symlinks and deleted paths are skipped), as `./`-prefixed relative paths, sorted and deduplicated. Outside a git checkout (or when git fails or lists nothing) the scanners walk the tree instead, skipping the directories `target`, `.git`, `node_modules` and `dist`.

```rust
#[must_use] pub fn rust_files(root: &Path) -> Vec<String>
#[must_use] pub fn text_files(root: &Path) -> Vec<String>
```

* `rust_files`: every `.rs` file.
* `text_files`: files with the extension `rs toml yml yaml json env sh fish md txt pem key cfg conf ini`, except names ending `Cargo.lock`.

## Symlinks

```rust
#[must_use] pub fn symlinks_leaving_the_project(root: &Path) -> Vec<(String, String)>
```

Tracked or untracked-unignored symlinks whose target is absolute, or a relative path that climbs out of the project (counting the link's own directory depth). Returns `(./path, target)`, sorted. Empty when git is unavailable.

## `unsafe`

```rust
#[must_use] pub fn line_uses_unsafe(line: &str) -> bool
#[must_use] pub fn files_using_unsafe(root: &Path) -> Vec<String>
#[must_use] pub fn allowlist(text: &str) -> Vec<String>
```

* `line_uses_unsafe`: false for lines starting (after whitespace) with `//`. Otherwise true if the line contains any of: `allow(unsafe_code)`, `forbid(unsafe_code)`, the keyword followed by ` {`, `{`, ` fn`, ` impl`, ` trait`, or `#[` + keyword + `(`. (A lone `unsafe(...)` inside `cfg_attr(...)` is not one of these patterns.)
* `files_using_unsafe`: the `.rs` files with at least one such line.
* `allowlist`: trimmed lines of an allowlist file without blanks and `#` comments.

## Secrets

```rust
#[must_use] pub fn line_leaks_secret(line: &str) -> Option<&'static str>
#[must_use] pub fn secrets_in(root: &Path) -> Vec<(String, usize, &'static str)>
```

`line_leaks_secret` returns one of: `a private key` (a `-----BEGIN ` line also containing `PRIVATE KEY-----`), `an AWS access key id` (`AKIA` + at least 16 of `A-Z0-9`), `a GitHub personal access token` (`ghp_` + at least 36 alphanumerics), `a Slack token` (`xoxb-`, `xoxa-`, `xoxp-`, `xoxr-`, `xoxs-` + at least 10 of alphanumerics and `-`). `secrets_in` scans `text_files`, skipping any path containing `/tests/`, `/testdata/`, `/fixtures/` or ending `.md`, and returns `(file, 1-based line, what)`.

## Running commands

```rust
#[derive(Debug)] pub struct Ran { pub success: bool, pub output: String }
impl Ran { #[must_use] pub fn tail(&self, lines: usize) -> String }
#[must_use] pub fn run(dir: &Path, program: &str, args: &[&str], env: &[(&str, &str)]) -> Ran
#[must_use] pub fn workspace_root() -> PathBuf
```

* `run`: executes `program args` in `dir` with extra environment variables and null stdin; `output` is stdout then stderr. If the program cannot start: `success: false` and `could not start `<program>`: <error> (is it installed?)`, so a missing tool is a failure, never a skip.
* `Ran::tail(n)`: the last `n` lines of output.
* `workspace_root`: the parent of `CARGO_MANIFEST_DIR` (the `quench-security` directory).

## Fuzz statistics

```rust
#[derive(Debug, PartialEq, Eq)] pub struct FuzzStats { pub execs: u64, pub coverage: u64, pub features: u64 }
#[must_use] pub fn fuzz_stats(output: &str) -> Option<FuzzStats>
```

Parses the last libFuzzer line of the form `#N DONE cov: C ft: F ...` (`execs = N`, `coverage = C`, `features = F`); `None` if there is none.
