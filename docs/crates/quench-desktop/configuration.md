---
title: Configuration
parent: quench-desktop
grand_parent: Crates
nav_order: 3
---

## Environment variables

| Variable | Read by | Meaning |
|---|---|---|
| `QUENCH_DESKTOP_CONTROL` | `run` (debug builds only) | Path of the control socket (see [internals](internals.md#control-socket)). |
| `QUENCH_DEV_URL` | the generated `main.rs`, not the crate | In debug builds, `App::dev(url)`. Set by `quench dev --linux`. |
| `DISPLAY` / `WAYLAND_DISPLAY` | GTK | Without a display `run` returns `RunError::NoDisplay`. |

## `App` fields

| Field | Default | Meaning |
|---|---|---|
| `title` | argument of `App::new` | Window title. |
| `width`, `height` | 1000, 700 | Initial size (`gtk_window_set_default_size`). |
| `bundle` | argument | The app's files. |
| `plugins` | empty | Added with `plugin`, `standard_plugins`, `with_echo`. |
| `dev_url` | none | Loaded instead of the bundle **only if `debug` is also true**; set by `dev()`. |
| `debug` | false | Web inspector (right-click, "Inspect Element"); enables `dev_url`. |

## Application id

`standard_plugins(id)`: the keyring attribute `app` and, via the window title, the notification app name. The menu entry and icon names come from `[linux] id` in `.quench.toml` (see [quench-tool configuration](../quench-tool/configuration.md)).

## System programs

`secret-tool` (libsecret-tools), `notify-send` (libnotify), `xdg-open` (xdg-utils): used by the plugins; each missing program makes its plugin answer `unavailable`.
