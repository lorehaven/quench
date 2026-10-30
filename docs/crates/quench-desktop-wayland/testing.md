---
title: Testing
parent: quench-desktop-wayland
grand_parent: Crates
nav_order: 5
---

```bash
cargo test -p quench-desktop-wayland
```

**Caution:** the test `a_window_opens_draws_and_closes_on_the_real_compositor` (`tests/unit/compositor_tests.rs`) starts the fixture binary on the real compositor when `WAYLAND_DISPLAY` is available (it opens a window for about a second; exit code 77 means no compositor and the test is skipped). In a desktop session, run the other suites with a filter or without a compositor variable set. Authors of this book did not run the tests.

| File | What it checks |
|---|---|
| `wire_tests.rs` | header layout, argument round trips, strings (NUL, padding), bad strings and short arguments, 24.8 fixed point, decoder with split input, invalid headers, size limit. |
| `sys_tests.rs` | bytes and a descriptor arrive together and the memory is shared; several descriptors in order; large sends; end of connection versus timeout; too many descriptors is an error; memfd zeroed, mapping rules; connect to a path. |
| `client_tests.rs` | request bytes, ids counting from the display, events decoded by interface, roundtrip, protocol error, closed or corrupt stream, deleted ids, roundtrip timeout, claiming descriptors. |
| `keymap_tests.rs` | US layout, caps lock, modifier masks, parsing a text keymap with levels and aliases, symbol names to text, keymaps written with numbers. |
| `window_tests.rs` | handshake against a fake compositor: naming and sizes, frames and damage, buffer release waits, ping and pointer, resize reallocation, close and frame callbacks, missing globals, no seat, keyboard through an announced layout, input method, clipboard, a seat announced before the first configure. |
| `compositor_tests.rs` | the fixture on a real compositor (skipped without one). |

The fake compositor tests use a `UnixStream` pair and need no display.

`tests/fixture/main.rs` (`quench-desktop-wayland-fixture`) draws a frame, prints what it saw, runs for `FIXTURE_SECONDS` (default 1) and exits 77 without a compositor.
