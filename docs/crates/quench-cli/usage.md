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
