---
title: quench-cli
parent: Crates
has_children: true
permalink: /crates/quench-cli/
nav_order: 8
---

`quench-cli` (library `quench_cli`) is the terminal side of the command-line tools: coloured text, banners and status lines, a REPL with a line editor and history, key events, raw mode, terminal size, a no-echo password prompt, and images in the terminal (kitty graphics protocol, sixel, and detection of what the terminal supports). It replaces `crossterm`, `rustyline` and `rpassword`. It has no `libc` and no `unsafe`: raw mode and size go through the system `stty`.

## Facts

| | |
|---|---|
| Version | 0.1.15 |
| Edition | 2024 |
| License | MIT |
| Platforms | everything but `keys`, `term`, `tty` and the functions `graphics::detect::detect` / `detect_in` is portable; those are `cfg(all(unix, feature = "tui"))` |
| Test harness | `harness = false` for the library and for the two integration tests (`unit`, `pty`); both use `quench_test::main!()` |
| External programs | `stty` (raw mode, size, no-echo input); the pty test also needs `script` |

```toml
[dependencies]
quench-cli = { version = "0.1.15", registry = "ennor" }
# without the line editor and terminal control:
# quench-cli = { version = "0.1.15", registry = "ennor", default-features = false }
```

## Features

| Feature | Default | Enables |
|---|---|---|
| `tui` | no | modules `keys`, `term`, `tty` (Unix only): key events, size, secret prompt, screen sequences, `RawMode`; and `graphics::detect::{detect, detect_in}` (asking the terminal in raw mode) |
| `repl` | yes | `editor` and `repl` modules; implies `tui` |

Always available (no feature): `style`, `terminal`, `require`, `graphics` (except the two detection functions), and `prelude`. The `prelude` re-exports `ReplControl` and `repl_run` only with `repl`.

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | `quench_core::env::var_os` for `NO_COLOR` and `PATH`; `quench_core::base64::STANDARD` for the kitty payload ([base64](../quench-core/api.md#base64)); the pty test uses `quench_core::tmp::TempDir` |
| `quench-test` (dev) | test harness |

## Modules

| Module | Gate | Page |
|---|---|---|
| `prelude` | none | [terminal and prelude](api-terminal.md) |
| `terminal` | none | [terminal and prelude](api-terminal.md) |
| `style` | none | [style](api-style.md) |
| `require` | none | [terminal and prelude](api-terminal.md#require) |
| `graphics` (`detect`, `kitty`, `reply`, `sixel`) | none; `detect::detect`/`detect_in` unix + `tui` | [graphics](api-graphics.md) |
| `editor`, `repl` | `repl` | [editor and REPL](api-editor.md) |
| `keys`, `term`, `tty` | unix + `tui` | [keys, term and tty](api-term.md) |

## Where to go next

* [Usage](usage.md)
* [Graphics](api-graphics.md): kitty, sixel, terminal replies, detection and `QUENCH_GRAPHICS`
* [Testing](testing.md)
* Laying out Markdown for the terminal (the text that graphics go next to): [quench-markdown-term](../quench-markdown-term/index.md)
* Parsing arguments for the same tool: [quench-args](../quench-args/index.md); the two together: [Building command-line tools](../../guides/building-cli-tools.md).

## Minimal example

```rust
use quench_cli::prelude::*;

print_box_banner("Riveter", "interactive REPL");
print_status(Tone::Success, "OK", "connected to switchboard");
let prompt = repl_prompt("riveter", "default");
assert!(prompt.contains("riveter") && prompt.ends_with("> "));
```

`print_box_banner` and `print_status` print to standard output; `NO_COLOR` (set and non-empty) turns the colour codes off everywhere in the crate. The graphics sequences are not colour codes, and `NO_COLOR` does not affect them.

## Environment variables

| Variable | Read by | Effect |
|---|---|---|
| `NO_COLOR` | `style`, `terminal` | set and non-empty: no SGR colour codes |
| `PATH` | `require` | where `require_binary` looks |
| `QUENCH_GRAPHICS` | `graphics::detect` | `none`, `kitty` or `sixel` (any case) forces the graphics answer; other values are ignored |
| `TERM`, `TMUX`, `STY` | `graphics::detect` | `TERM` unset, empty or `dumb`: no probing; `TMUX`/`STY` non-empty, or `TERM` starting with `tmux`/`screen`: treated as a multiplexer: nothing is sent; no graphics unless forced, and a forced protocol comes with no sizes or colours |
