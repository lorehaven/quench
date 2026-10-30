---
title: Testing
parent: quench-android-surface
grand_parent: Crates
nav_order: 4
---

```bash
cargo test -p quench-android-surface
```

All tests are in `tests/unit.rs` (custom harness, `quench_test::main!()`); they run the program over in-memory streams with a real `NativeRenderer`, so no device is needed. Covered: every event round-trips through `encode_event`/`read_event`; malformed messages are errors; a frame goes out as RGBA only when something changed; tap versus drag; `touch-action` directions; fling start, slow-down and stop; density scaling of slop; keys, text and soft-keyboard follow-focus; Back used by the page or leaving; accessibility actions, colour scheme and visibility; the flattened tree (parents, flags, strings); message lengths and kinds; running over a pair of streams until the host closes them; plugin calls, answers and events; the clipboard both ways; long-press selection and handles; the press delay.

Not covered by unit tests (verified only by the emulator script `scripts/android-surface-e2e.fish`, outside this book): the Kotlin `SurfaceView`, `InputConnection`, `AccessibilityNodeProvider`, the `ClipboardManager` code, TalkBack, a physical device.
