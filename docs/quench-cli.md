# Quench CLI

`quench-cli` (crate `quench_cli`) is a small shared terminal-UI library used by Forge's CLI and REPL tools, so they present one consistent style (colors, banners, status lines, prompts) instead of each rolling its own ANSI-code handling. It has no dependencies of its own beyond `libc` on Unix (for the REPL's raw terminal mode). Consumers across the workspace include `anvil`, `riveter`, `pulley`, `warehouse-cli`, `forge-toolbox`, `welder`, `conveyor-cli`, `foreman`, and the `quench-starter`/`quench-client` libraries.

## Features

`tui` (Unix) is terminal control on `libc`: key events (`keys`), size, a no-echo password prompt and screen sequences (`term`). `repl` (default, includes `tui`) adds the line editor (`editor`), the REPL loop (`repl`, `prelude::repl_run`) and the termios raw-mode guard. Without either, `quench-cli` has no dependencies at all: styling (`style`), the terminal helpers (`print_box_banner`, `print_status`, `repl_prompt`, ...) and `require_binary` remain.

## Public API / Key Types

All in `quench_cli::terminal` (`libs/quench-cli/src/terminal.rs`):

- `print_box_banner(title, subtitle)` — draws a bordered box banner.
- `print_status(tone, label, message)` — prints a colored status line; `tone` is a `Tone` (`Info`, `Success`, `Warn`, `Error`).
- `print_component_preview(name, description)` — prints a `[name] description` line.
- `repl_prompt(app, context) -> String` — formats a REPL prompt like `app(context)> `.
- `print_line(message)` / `print_error_line(message)` — println/eprintln wrappers taking `impl AsRef<str>`.
- `print_inline(message)` — prints without a trailing newline and flushes stdout immediately.
- Color/formatting constants: `RESET`, `BOLD`, `DIM`, `CYAN`, `BLUE`, `GREEN`, `YELLOW`, `WHITE` (raw ANSI escape sequences), plus `SEP` and `SEP_THIN` (horizontal separator strings).

`quench_cli::prelude` re-exports the subset most callers need: `print_box_banner`, `print_status`, `print_line`, `repl_prompt`, `Tone`, and the color/separator constants. `print_component_preview`, `print_error_line`, and `print_inline` are reachable via `quench_cli::terminal::*` but not re-exported from `prelude`.

## Terminal control (`tui`)

What the full-screen tools used `crossterm` and `rpassword` for.

- **`style`** (always on) — `Stylize` gives any displayable value `.bold()`, `.dim()`, `.underlined()` and the sixteen colours with crossterm's names: `.red()` `.green()` `.yellow()` `.blue()` `.magenta()` `.cyan()` `.white()` are the bright ones, `.dark_red()` ... `.dark_cyan()` the normal ones, `.black()`, `.dark_grey()`, `.grey()`; chaining adjusts one `Styled`, which displays wrapped in ANSI codes. `NO_COLOR` (non-empty) turns the codes off.
- **`keys`** — `Keys::stdin()` / `Keys::on_fd(fd)` with `poll(timeout)` (is a key ready?) and `read()`. A `KeyEvent` is a `KeyCode` (`Char`, `Enter`, `Esc`, `Backspace`, `Tab`, `BackTab`, arrows, `Home`/`End`, `PageUp`/`PageDown`, `Insert`/`Delete`, `F(1..=12)`) with `Modifiers { ctrl, alt, shift }`. A lone Escape is told apart from the start of a sequence by a 30 ms wait; unknown sequences are skipped. `parse_key(bytes)` is the pure decoder. Pair it with `RawMode` (below) - there are no key-release events, every event is a press.
- **`term`** — `is_tty(fd)` / `stdin_is_tty()` / `stdout_is_tty()`, `size()` (columns, rows), the escape sequences `ENTER_ALTERNATE_SCREEN`, `LEAVE_ALTERNATE_SCREEN`, `CLEAR_ALL`, `CLEAR_LINE`, `HIDE_CURSOR`, `SHOW_CURSOR`, `move_to(column, row)` and `clear_screen(out)`, and `prompt_password(prompt)` / `read_secret(fd, prompt)`: the prompt goes to standard error, echo is off while the line is read, and a descriptor that is not a terminal is read as it is.

- **`tty::RawMode`** — `RawMode::enable(fd)` puts a terminal in raw mode (no echo, no line buffering, no keyboard signals, no CR translation) and restores it when dropped; `None` for a descriptor that is not a terminal. `enable_keeping_input(fd)` is the same without discarding unread input, for a program that polls for keys between other work.

Unix only; tested through pseudo-terminals (echo really is off, sizes round-trip, keys arrive in pieces).

## REPL line editor

`quench_cli::editor::LineEditor` reads one line with editing and history, replacing `rustyline`: UTF-8 text, Backspace/Delete, the arrow keys, Home/End, word movement (Alt-B/F, Ctrl-arrows), Ctrl-A/E/B/F/K/U/W/T/L/P/N, Up/Down through history, Ctrl-C (`Outcome::Interrupted`) and Ctrl-D (`Outcome::Eof` on an empty line, delete otherwise). The prompt may contain ANSI colours; only its visible width is used to place the cursor. The line is a single row (no completion, no reverse search, no wrapping-aware cursor math). When stdin is not a terminal it prints the prompt and reads a plain line. `quench_cli::repl::run(prompt, |line| ReplControl::Continue(next_prompt))` is the loop most tools use; `run_with` takes an editor you keep.

`edit_line` is the same logic over any byte stream, which is how it is tested without a terminal; the raw-mode switch itself (`libc` termios, restored on drop) is tested through a pseudo-terminal.

## Usage example

```rust
use quench_cli::prelude::*;

print_box_banner("Riveter", "interactive REPL");
print_status(Tone::Success, "OK", "connected to switchboard");
let prompt = repl_prompt("riveter", "default");
```

## Integration checks

The editor, the REPL loop, the terminal helpers and the raw-mode switch have their own tests (`cargo test -p quench-cli`). For the styling, also run a consumer:

```bash
cargo run -p anvil -- --help
cargo run -p riveter -- repl
cargo run -p pulley
cargo run -p warehouse-cli -- --help
cargo run -p forge-toolbox
cargo run -p welder -- --workflow ./welder/samples/agent.toml
```

[Home](../README.md)
