---
title: Testing
parent: quench-desktop
grand_parent: Crates
nav_order: 6
---

```bash
cargo test -p quench-desktop                      # no window: bundle, bridge, plugins, origins
cargo test -p quench-desktop --features window    # also starts a real window via the fixture
```

**Do not run the `window` suite in a desktop session you are using, and not at all where a dbus trap exists** (it opens windows; `tests/unit/window_tests.rs` starts `quench-desktop-fixture`, which exits 77 without a display and the tests then skip). This book's authors ran neither.

`tests/unit/`:

| File | Checks |
|---|---|
| `host_tests.rs` | `origin_of`, `is_app_origin`, `opens_outside` |
| `bundle_tests.rs` | root and route fallbacks, content types, 404s for other origins and hostile paths, `from_dir` |
| `bridge_tests.rs` | answers by id, rejection codes, dropped messages, panicking plugin, late answers from another thread, events |
| `plugin_tests.rs` | each plugin against a fake `Runner` / fake `/sys` / fake `Chooser`: secrets on stdin, notifications, reminders, share filtering, files grants and chunking, a named pipe that cannot hang, the real keyring when present |
| `window_tests.rs` (feature `window`) | a window serving its scheme and round-tripping messages; the host answering plugin calls; a throwing script; links leaving the app; the standard plugins through the real bridge |

`tests/fixture/main.rs` is the window program the `window_tests` drive (modes selected by `FIXTURE`; `FIXTURE=app` runs `quench_desktop::run` with the echo plugin and the control socket).

`QUENCH_DESKTOP_REAL=1` makes one `plugin_tests.rs` test use the **real** keyring (`secret-tool`) and notification service (`notify-send`) under a throwaway application id; without it that test prints that it is not trying them. Do not set it in a session you do not want a notification in.
