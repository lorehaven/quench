---
title: Testing
parent: quench-desktop-native
grand_parent: Crates
nav_order: 4
---

```bash
cargo test -p quench-desktop-native
```

`tests/unit.rs` has four tests. Three use `Controller` with synthetic `WindowEvent`s and need no compositor. `the_loop_draws_the_page_and_keeps_running_without_input` **opens a real window** (200 x 100) with `Host::open` on whatever compositor is running, steps the loop, types into a text field and passes (printing "skipped") when no compositor is available. Do not run the whole suite inside a desktop session you are using; run a filter such as `cargo test -p quench-desktop-native -- a_key_press` (see the harness's filter syntax) or run it under a headless compositor.

Covered: a key press becomes a DOM key event with its modifiers; the loop draws the page and keeps running without input; window events drive a page the way a user would (pointer, clicks, wheel, typing, key repeat, input method).
