---
title: Usage
parent: quench-cli
grand_parent: Crates
nav_order: 1
---

## 1. Banner, status lines, prompts

```rust
use quench_cli::prelude::*;

print_box_banner("Anvil", "build and release");
print_status(Tone::Info, "INFO", "reading .anvil/");
print_status(Tone::Error, "FAIL", "lint found 3 problems");
print_line(format!("{SEP}"));
```

`print_status` writes to standard output. For errors that belong on standard error, use `quench_cli::terminal::print_error_line`. Pitfall: `Tone` is not `Clone`; build a new value per call.

## 2. Coloured fragments inside a line

```rust
use quench_cli::style::Stylize;

let line = format!("{} {}", "deploy".bold().green(), "web".dim());
println!("{line}");
assert!(line.starts_with("\x1b[1;92mdeploy\x1b[0m") || std::env::var_os("NO_COLOR").is_some());
```

`NO_COLOR` set to anything non-empty makes every `Styled` and every `terminal::print_*` helper emit plain text (the raw constants `BOLD`, `CYAN`... are not affected). Padding: format the plain string first, then style it, because padding is not applied around the codes.

## 3. Check external tools first

```rust
use quench_cli::prelude::*;

fn main() -> Result<(), MissingBinary> {
    require_binary("git", "git is needed to read the repository")?;
    Ok(())
}
```

A missing tool prints one line: `` `git` not found on PATH. git is needed to read the repository `` even through `main`'s `{:?}` error path.

## 4. A REPL

```rust,no_run
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

Ctrl-C and Ctrl-D end the loop with `Ok(())`. History lives for the process; to keep or preload it, create a `LineEditor`, call `add_history_entry` for each saved line and use `quench_cli::repl::run_with(&mut editor, prompt, handler)`. With stdin not a terminal the prompt is printed and plain lines are read, so the same tool works in a pipe.

## 5. Parse each REPL line with quench-args

`quench-args` parses a list of words, so a REPL can reuse a `Parser`. Use `Cli::try_parse_from` with a program name first; on an error print `e.render()` and continue; on `e.is_display()` print help. A complete version is in the [guide](../../guides/building-cli-tools.md).

## 6. A secret from the terminal

```rust,no_run
let password = quench_cli::term::prompt_password("Password: ")?;
# Ok::<(), std::io::Error>(())
```

The prompt goes to standard error, echo is off, a newline is printed afterwards. Needs the `tui` feature (default) and Unix.

## 7. A full-screen loop

See the `Keys` example in [keys, term and tty](api-term.md#struct-keys): enable raw mode, print `ENTER_ALTERNATE_SCREEN` and `HIDE_CURSOR`, loop on `poll`/`read`, and print `SHOW_CURSOR` and `LEAVE_ALTERNATE_SCREEN` before returning. `RawMode` restores the terminal on drop, including when the loop returns early with `?`; a panic that aborts does not run `Drop`.

## 8. A picture or formula in the terminal

Goal: show an image inline where the terminal can draw one, and text where it cannot. Ask the terminal once at start-up, then pick the protocol. See [graphics](api-graphics.md) for every item.

```rust,no_run
use quench_cli::graphics::detect::{DEFAULT_TIMEOUT, Protocol, detect};
use quench_cli::graphics::{Rgb, kitty, sixel};

fn show(png: &[u8], rgba: &[u8], width: usize, height: usize, text: &str)
    -> Result<(), sixel::SixelError>
{
    let caps = detect(DEFAULT_TIMEOUT);           // unix + `tui`; never fails
    match caps.protocol() {
        Some(Protocol::Kitty) => print!("{}", kitty::transmit_png(png, 1, kitty::Placement::cells(12, 1))),
        Some(Protocol::Sixel) => {
            let background = caps.background.unwrap_or(Rgb::BLACK);
            print!("{}", sixel::encode(rgba, width, height, background)?);
        }
        None => print!("{text}"),
    }
    Ok(())
}
```

The PNG for kitty and the RGBA pixels for sixel come from elsewhere, for example [quench-image](../quench-image/index.md) (`decode`, `encode_png`). Sixel needs RGBA pixels; kitty takes the PNG file as it is. `caps.cell_size_px` tells how many pixels one cell has, so you can size the image to whole cells.

Kitty keeps the image under its id. To show it again (after scrolling, say), print `kitty::place(1, placement)`. To free it, print `kitty::delete(1)`.

Pitfalls:

* Call `detect` once, before the line editor or key reader starts. The probe takes up to about 200 ms, and keys typed during it are lost. A reply that arrives later than the timeout ends up in the program's input.
* Nothing is asked, and `protocol()` is `None`, when stdin or stdout is not a terminal, when `TERM` is `dumb` or unset, and inside tmux or screen. Set `QUENCH_GRAPHICS=kitty`, `sixel` or `none` to override the answer. Inside tmux or screen the queries are never sent, even with an override: the forced protocol comes back with `cell_size_px` and `background` set to `None`, so have a default for them. The program does not wrap the image sequences for tmux passthrough either.
* `detect` flushes standard output and standard error and writes its queries through Rust's `stdout()`, so text you printed before it (even `print!` without a newline) shows before the queries. Output written to descriptor 1 some other way (a `File` on `/dev/stdout`, a child process) is not ordered by this.
* For sixel, blend onto the real background. A formula antialiased onto black looks wrong on a light terminal. `caps.background` is the terminal's `OSC 11` answer, or `None`.
