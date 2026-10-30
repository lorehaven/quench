---
title: Testing
parent: quench-android
grand_parent: Crates
nav_order: 5
---

Tests live in `quench-android/tests/unit/` and run through the workspace's own harness (`quench_test::main!()`, `harness = false`).

```bash
cargo test -p quench-android
```

| File | What it checks |
|---|---|
| `serve_tests.rs` | status byte and payload; request = operation + input; requests answered in order until input ends; inputs larger than a pipe buffer; unknown operation, bad name and panic become error answers and serving continues; re-registering replaces a handler; truncated and oversized requests are errors; an unwritable output ends the session. |
| `tz_tests.rs` | next reading today or tomorrow; daily reminder keeps its clock time through both DST changes; the zone decides, not the device; skipped readings; repeated readings; descriptive errors; the JSON operation. |

No device is needed. Testing the Kotlin side requires the emulator scripts of the repository (`scripts/android-plugins-e2e.fish`), which this book does not cover.
