---
title: Testing
parent: quench-tool
grand_parent: Crates
nav_order: 7
---

```bash
cargo test -p quench-tool
```

Tests are in `quench-tool/tests/unit/` (custom harness, `quench_test::main!()`); about 160 test and helper functions. They use a recording fake `System` (`support.rs`), temporary directories and the pure functions; they do not run cargo for the project, Gradle, adb or a browser. A few run harmless real commands through `system::Host` (`system_tests.rs`).

| File | Covers |
|---|---|
| `cli_tests.rs` | argument parsing, help and version, exit codes |
| `scaffold_tests.rs` | `quench new`: files, names, templates, `--quench-path` |
| `project_tests.rs` | `.quench.toml`, name rules, workspace detection (flat layout) |
| `release_tests.rs`, `dev_tests.rs`, `watch_tests.rs`, `browser_tests.rs` | build, bundle, dev loop, watcher, `test --browser` |
| `android_tests.rs`, `apk_tests.rs` | android init, keystore, launcher, build-native, APK/AAB build, signing environment, versions |
| `plugins_tests.rs`, `native_tests.rs` | each plugin's files, manifest edits, registration, update |
| `linux_tests.rs`, `packaging_tests.rs` | `desktop/`, deb, rpm, arch, source contents |
| `doctor_tests.rs`, `system_tests.rs` | doctor checks and report; the real `System` |

`versions.rs` is kept on the same minor version as the workspace's crates by a test.
