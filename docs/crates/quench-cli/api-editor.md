---
title: Editor and REPL
parent: quench-cli
grand_parent: Crates
nav_order: 2
---

Modules `editor` and `repl`, both behind feature `repl` (default). `LineEditor::readline` uses `tty::RawMode` on Unix.

## `editor`

A small single-line editor with history, replacing `rustyline`. The line is one row: no wrapping-aware cursor maths, no completion, no reverse search.

### `enum Outcome`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Line(String),   // a line, without its newline
    Interrupted,    // Ctrl-C
    Eof,            // Ctrl-D on an empty line, or the end of the input
}
```

### `struct LineEditor`

```rust
#[derive(Debug, Default)]
pub struct LineEditor { /* history: Vec<String> */ }

impl LineEditor {
    #[must_use] pub const fn new() -> Self
    pub fn add_history_entry(&mut self, line: impl Into<String>) -> bool
    #[must_use] pub fn history(&self) -> &[String]
    pub fn readline(&mut self, prompt: &str) -> io::Result<Outcome>
}
```

* `add_history_entry`: returns `false` (and does nothing) when `line.trim()` is empty or the line equals the last entry; otherwise appends it as given (no trimming) and returns `true`. History is in memory only; there is no file and no size limit.
* `history()`: entries oldest first.
* `readline(prompt)`: on Unix, when standard input is a terminal, switches it to raw mode (`RawMode::enable(0)`), runs `edit_line` on locked stdin/stdout and restores the terminal when it returns. Otherwise (pipe, file, closed stdin, non-Unix) it writes the prompt to stdout, flushes, and reads one line with `read_line`: zero bytes read gives `Outcome::Eof`, else `Outcome::Line` with the trailing `\n`/`\r` removed. Errors: any I/O error, including a failing `stty`.

### `fn visible_width`

```rust
#[must_use]
pub fn visible_width(text: &str) -> usize
```

Width of `text` in characters, ignoring CSI escape sequences (`ESC [ ... final byte` with the final byte in `@`..=`~`) and control characters. Counts `char`s, not display cells (wide characters count 1).

### `fn edit_line`

```rust
pub fn edit_line<R: Read, W: Write>(
    history: &mut [String],
    prompt: &str,
    input: &mut R,
    output: &mut W,
) -> io::Result<Outcome>
```

The editing logic over any byte stream in raw form, which is how it is tested. It redraws the whole line after each key with `\r\x1b[K{prompt}{text}\r` and then moves right by the visible width of prompt plus cursor position (`\x1b[<n>C`). `history` is browsed with Up/Down but not modified (the slice is `&mut` but only read); record the result with `add_history_entry`.

Ends: Enter writes `\r\n` and returns `Line`; Ctrl-C writes `^C\r\n` and returns `Interrupted`; Ctrl-D on an empty line writes `\r\n` and returns `Eof`; the end of `input` writes `\r\n` and returns `Eof` if the line is empty, else `Line` with what was typed so far.

Key bindings:

| Keys | Action |
|---|---|
| printable text, UTF-8 | insert at the cursor |
| Enter (`\r` or `\n`) | accept |
| Backspace (0x7f or 0x08) | delete before cursor |
| Delete (`ESC [ 3 ~`), Ctrl-D (non-empty line) | delete under cursor |
| Left / Ctrl-B, Right / Ctrl-F | move one character |
| Home / Ctrl-A (`ESC [ H`, `ESC [ 1 ~`, `ESC [ 7 ~`, `ESC O H`) | start of line |
| End / Ctrl-E (`ESC [ F`, `ESC [ 4 ~`, `ESC [ 8 ~`, `ESC O F`) | end of line |
| Alt-B, Ctrl-Left or Alt-Left (`ESC [ 1;5 D`, `ESC [ 1;3 D`) | word left |
| Alt-F, Ctrl-Right or Alt-Right (`ESC [ 1;5 C`, `ESC [ 1;3 C`) | word right |
| Ctrl-K | delete to end of line |
| Ctrl-U | delete to start of line |
| Ctrl-W | delete the word before the cursor (whitespace-delimited) |
| Ctrl-T | transpose the two characters around the cursor (at the end of the line: the last two); no effect at the start or on a one-character line |
| Ctrl-L | write `\x1b[H\x1b[2J` (clear screen), then redraw |
| Up / Ctrl-P, Down / Ctrl-N | history; the line typed so far is kept as a draft and restored when going past the newest entry |
| Ctrl-C | interrupt |

Anything else (unknown escape sequences, other control keys, invalid UTF-8, an escape at the end of input) is ignored. Bare Escape is not a key here (`ESC` followed by `b`/`f` is Alt-B/F).

## `repl`

### `enum ReplControl`

```rust
#[derive(Debug)]
pub enum ReplControl {
    Continue(String),   // keep going, with this prompt next
    Exit,
}
```

The prompt travels with the control value instead of a separate closure so that a self-borrowing handler and prompt never coexist.

### `fn run`

```rust
pub fn run(
    initial_prompt: impl Into<String>,
    on_line: impl FnMut(&str) -> ReplControl,
) -> std::io::Result<()>
```

`run_with(&mut LineEditor::new(), initial_prompt, on_line)`. Re-exported as `quench_cli::prelude::repl_run`.

### `fn run_with`

```rust
pub fn run_with(
    editor: &mut LineEditor,
    initial_prompt: impl Into<String>,
    mut on_line: impl FnMut(&str) -> ReplControl,
) -> std::io::Result<()>
```

Loop: `editor.readline(&prompt)?`; on `Interrupted` or `Eof` the loop ends and the function returns `Ok(())`. The line is trimmed; an empty one is skipped (no history, no callback). A non-empty one is added to history (`add_history_entry`) and passed trimmed to `on_line`; `Continue(p)` sets the prompt to `p`, `Exit` ends the loop. Error reporting stays with `on_line`. Returns an I/O error from the terminal.

Because the loop treats Ctrl-C like Ctrl-D, a REPL built on it has no "clear the line" meaning for Ctrl-C.

```rust,no_run
use quench_cli::prelude::*;

fn main() -> std::io::Result<()> {
    repl_run(repl_prompt("tool", "main"), |line| {
        match line {
            "quit" => return ReplControl::Exit,
            other => print_line(format!("you said: {other}")),
        }
        ReplControl::Continue(repl_prompt("tool", "main"))
    })
}
```

With standard input closed or at end (as under `cargo test`), `run` returns `Ok(())` at once without calling the handler (`tests/unit/repl_tests.rs`).
