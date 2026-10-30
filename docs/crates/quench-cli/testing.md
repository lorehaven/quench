---
title: Testing
parent: quench-cli
grand_parent: Crates
nav_order: 6
---

## How the crate is tested

All test targets set `harness = false` and call `quench_test::main!()` (see [quench-test](../quench-test/index.md)).

| Target | File | Covers |
|---|---|---|
| lib unit tests | inside `src/` (`style.rs`, `keys.rs`, `editor.rs`, `tty.rs`, `term.rs`) | colour codes and `NO_COLOR`; key decoding, the `Keys` reader and its Escape timing; every editor binding through `edit_line` on byte slices; pipes are not terminals |
| `unit` (`tests/unit.rs`) | `tests/unit/terminal_tests.rs`, `repl_tests.rs` (needs `repl`), `require_tests.rs` | printing helpers, constants, `repl_prompt`; `run` returning at once on end of input; `require_binary` for `sh` and a bogus name |
| `pty` (`tests/pty.rs`, `required-features = ["repl"]`) | | the real terminal code under a pseudo-terminal |

The `pty` test re-runs its own binary as a child under `script -qfec` (Linux) so that stdin and stdout are a pty, types into `script`, and reads the outcome from a file named by the environment variable `QUENCH_CLI_PTY_OUTCOME_FILE`. It checks: `is_tty`, `size_of` after `stty rows 30 cols 100`, raw mode on and restored, `read_secret` without echo, a typed line with history, Ctrl-C and Ctrl-D, and the plain fallback with a pipe. It needs `script` and `stty` on `PATH`.

## Running

```bash
cargo test -p quench-cli </dev/null
```

Close standard input (`</dev/null`): the REPL test relies on end-of-input stdin, and an interactive stdin would make `LineEditor::readline` wait for you.

## Testing code that uses it

* Test line handling with `editor::edit_line(&mut history, "> ", &mut bytes.as_slice(), &mut output)`; no terminal needed.
* Test key handling with `keys::parse_key(b"\x1b[1;5C")`.
* Keep `NO_COLOR` handling in one test: it is process-wide state (the crate's own style test does this).
