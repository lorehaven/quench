# quench-cli
CLI and REPL toolkit with a unified toolbox-style terminal UI.

## What it is
`quench-cli` is the terminal side of the command-line tools: coloured text, banners and status lines, a REPL with a line editor and history, key events, raw mode, terminal size and a no-echo password prompt. It replaces `crossterm`, `rustyline` and `rpassword`, with no `libc` and no `unsafe`: raw mode and size go through the system `stty`.

It pairs with `quench-args`, which parses a list of words, so a REPL can reuse a `Parser` for each line.

## Use cases
- Print banners and status lines in a tool's output.
- Colour fragments of a line with `Stylize` (honours `NO_COLOR`).
- Check that external binaries exist with `require_binary`.
- Run an interactive REPL with line editing and history.

## Example
```rust
use quench_cli::prelude::*;

fn main() -> std::io::Result<()> {
    print_box_banner("Riveter", "interactive REPL");
    let mut env = String::from("default");
    repl_run(repl_prompt("riveter", &env), |line| {
        match line {
            "quit" | "exit" => return ReplControl::Exit,
            other if other.starts_with("env ") => env = other[4..].to_string(),
            other => print_status(Tone::Warn, "?", other),
        }
        ReplControl::Continue(repl_prompt("riveter", &env))
    })
}
```

## Features
- `repl` (default): the `editor` and `repl` modules; implies `tui`.
- `tui`: key events, terminal size, secret prompt, screen sequences and `RawMode` (Unix only).

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-cli/
