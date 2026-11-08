---
title: Testing
parent: quench-cli
grand_parent: Crates
nav_order: 7
---

## How the crate is tested

All test targets set `harness = false` and call `quench_test::main!()` (see [quench-test](../quench-test/index.md)).

| Target | File | Covers |
|---|---|---|
| lib unit tests | inside `src/` (`style.rs`, `keys.rs`, `editor.rs`, `tty.rs`, `term.rs`) | colour codes and `NO_COLOR`; key decoding, the `Keys` reader and its Escape timing; every editor binding through `edit_line` on byte slices; pipes are not terminals (`RawMode::enable`, `enable_keeping_input` and `enable_with_read_timeout` return `None` for a pipe and for `-1`) |
| `unit` (`tests/unit.rs`) | `tests/unit/graphics_tests.rs`, `terminal_tests.rs`, `repl_tests.rs` (needs `repl`), `require_tests.rs` | the graphics module (below); printing helpers, constants, `repl_prompt`; `run` returning at once on end of input; `require_binary` for `sh` and a bogus name |
| `pty` (`tests/pty.rs`, `required-features = ["repl"]`) | | the real terminal code under a pseudo-terminal |

The `pty` test re-runs its own binary as a child under `script -qfec` (Linux) so that stdin and stdout are a pty, types into `script`, and reads the outcome from a file named by the environment variable `QUENCH_CLI_PTY_OUTCOME_FILE`. It checks: `is_tty`, `size_of` after `stty rows 30 cols 100`, raw mode on and restored, `read_secret` without echo, a typed line with history, Ctrl-C and Ctrl-D, the plain fallback with a pipe, and graphics detection: the child prints text with `print!` and no newline, then calls `detect_in` (raw mode with `min 0 time 1`, nothing answers, `None` after the 200 ms timeout), and the parent checks that the text reached the screen before the kitty query. It needs `script` and `stty` on `PATH`.

## The graphics tests

`tests/unit/graphics_tests.rs` has no terminal. Everything runs on fixed bytes:

| Area | What is checked |
|---|---|
| kitty | exact sequences for a small PNG, with and without `c`/`r`/`C`; chunking at 4096 bytes of base64 (1, 2 and 4 chunks: keys only on the first, `m=1` ... `m=0`, the joined payload decodes back); `place`, `delete`, `QUERY` |
| sixel | exact strings for one pixel, two colours across two bands, a run of 100 (`!100`) and short runs written out, transparent pixels and holes; a test decoder parses the output back and compares every pixel with `blend` + `as_shown`; an antialiased "formula" of one colour gets an exact palette of 4 entries; a full alpha ramp of one grey needs at most 101 entries, and on a tinted background stays exact and under 256; a 64×64 image of distinct colours goes through median cut to exactly 256 entries, each pixel within 24 per channel of the original (at most 8 entries with `max_colours = 8`); the wrong buffer size; `blend` and `as_shown` values |
| replies | canned answers of kitty, WezTerm, foot, xterm (window ops off, BEL-terminated colours) and VTE; each single parser's accepted and rejected forms; replies mixed with typed keys, a kitty error, a DA2 reply and a DCS; an unfinished OSC, APC or DCS (cut by a new `ESC`, CAN or SUB) followed by complete replies, which are still found, and one still open at the end; the 8-bit ST 0x9c and the introducers 0x9b, 0x9d, 0x9f, 0x90; UTF-8 text with 0x9b/0x9c/0x9d continuation bytes not taken for controls; every prefix and suffix of the canned answers and 2000 pseudo-random inputs never panic |
| detection | `probe` against a canned reader (one chunk per read, then `Ok(0)`) and a clock that moves 10 ms per call: the bytes written are exactly `QUERIES`; answers cut mid-sequence are put together; reading stops at the DA1 reply (later chunks stay unread); answers without the sentinel are kept; `Interrupted`/`WouldBlock` retried, `UnexpectedEof` stops, other errors and a broken writer fail. Giving up: a silent terminal whose reads each take 100 ms (the tty read timeout, on a shared fake clock) is read twice and the probe ends at 200 ms with 3 clock calls and no sleeping; an input at its end (`Ok(0)` or `WouldBlock` at once, forever) on the real clock with a 100 ms timeout takes 2 to 11 reads and ends between 100 ms and 1 s; the same on the 10 ms ticking clock takes 10 reads for 200 ms. The cell size derived from the text area; `plan` for no terminal, `TERM` dumb/empty/unset, tmux and screen with and without an override (never probing; a forced protocol with no sizes or colours); `Forced::parse` and `resolve` with each override; `detect_in` with `terminal: false` sends nothing (needs unix + `tui`) |

Nothing here talks to a real terminal. The replies are canned copies of what those terminals send. Real terminals' answers and timing are not covered by any automated test. The `pty` test runs `detect_in` once on a pseudo-terminal that never answers (see above).

## The REPL test

`run_returns_ok_immediately_on_eof_stdin_without_calling_on_line` (`tests/unit/repl_tests.rs`) runs itself in a child process. The parent re-runs the test binary (`std::env::current_exe()`) with the test's name as its argument, the environment variable `QUENCH_CLI_REPL_EOF_CHILD=1`, and stdin, stdout and stderr on `/dev/null`. It then waits up to 60 seconds for the child to succeed, and fails with "the REPL did not stop at the end of its input" otherwise. The child calls `repl::run` and checks that it returns `Ok` without calling the handler.

Earlier versions read the test runner's real stdin. They hung where stdin never ends (a socket, as some CI and agent runners provide) and waited for typing at a terminal.

## Running

```bash
cargo test -p quench-cli
cargo test -p quench-cli --no-default-features   # graphics without `tui`: the detect_in test is left out
```

Standard input no longer matters: the REPL test gives its child `/dev/null` itself. The `pty` test needs `script` and `stty`.

## Testing code that uses it

* Test line handling with `editor::edit_line(&mut history, "> ", &mut bytes.as_slice(), &mut output)`; no terminal needed.
* Test key handling with `keys::parse_key(b"\x1b[1;5C")`.
* Keep `NO_COLOR` handling in one test: it is process-wide state (the crate's own style test does this).
* Test graphics detection without a terminal:
  * Build an `Environment` by hand instead of `Environment::from_process()`, and check `plan(&env)`.
  * Feed `probe` a reader of canned replies and a fake clock (`&mut || elapsed`).
  * Pass the result to `resolve`.
  * `detect_in(&env, timeout)` with `terminal: false` never touches the terminal.
* Set `QUENCH_GRAPHICS=kitty|sixel|none` to force a protocol in end-to-end runs. With `none`, nothing is written to the terminal.
