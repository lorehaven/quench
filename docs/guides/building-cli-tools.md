---
title: Building command-line tools
parent: Guides
nav_order: 8
---

How to build a command-line tool with [quench-args](../crates/quench-args/index.md) (parsing) and [quench-cli](../crates/quench-cli/index.md) (output and REPL), test it with [quench-test](../crates/quench-test/index.md), and avoid the traps. The program below is complete; it was compiled and run, and the outputs shown are real.

## What each crate does

| Need | Crate | Items used here |
|---|---|---|
| parse `argv` into types, help, errors, exit codes | `quench-args` | `Parser`, `Subcommand`, `ValueEnum`, `#[command]`, `#[arg]` |
| colours, banner, status lines, prompt, REPL | `quench-cli` | `prelude` (`print_status`, `Tone`, `print_box_banner`, `repl_prompt`, `repl_run`, `ReplControl`, `print_line`), `style::Stylize`, `terminal::print_error_line` |
| run the built binary in a test | `quench-test` | `cmd::Command`, `predicate` |

```toml
[dependencies]
quench-args = { workspace = true }
quench-cli = { workspace = true }          # default features include the REPL
```

## The program

```rust
use quench_args::{Parser, Subcommand, ValueEnum};
use quench_cli::prelude::*;
use quench_cli::style::Stylize;
use quench_cli::terminal::print_error_line;
use std::path::{Path, PathBuf};

/// A tiny note keeper.
#[derive(Parser, Debug)]
#[command(name = "notes", version = "0.1.0")]
struct Cli {
    /// File that holds the notes.
    #[arg(long, short = 'f', global = true, value_name = "PATH", default_value = "notes.txt")]
    file: PathBuf,
    /// Print more detail.
    #[arg(long, short = 'v', global = true)]
    verbose: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Add a note
    #[command(visible_alias = "a")]
    Add {
        /// How important the note is.
        #[arg(long, value_enum, default_value_t = Priority::Normal)]
        priority: Priority,
        /// The words of the note.
        #[arg(required = true, value_name = "WORD")]
        text: Vec<String>,
    },
    /// Show the notes
    List {
        /// Show at most N notes (all when N is left out: `--limit`).
        #[arg(long, value_name = "N", num_args = 0..=1, require_equals = true, default_missing_value = "10")]
        limit: Option<usize>,
    },
    /// Read commands from the terminal
    Repl,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Priority {
    Low,
    Normal,
    High,
}

impl Priority {
    fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
        }
    }
}

fn add(file: &Path, priority: Priority, text: &[String]) -> std::io::Result<()> {
    let mut notes = std::fs::read_to_string(file).unwrap_or_default();
    notes.push_str(&format!("{}\t{}\n", priority.name(), text.join(" ")));
    std::fs::write(file, notes)
}

fn list(file: &Path, limit: Option<usize>) {
    let notes = std::fs::read_to_string(file).unwrap_or_default();
    let shown = notes.lines().take(limit.unwrap_or(usize::MAX));
    let mut count = 0;
    for line in shown {
        let (priority, text) = line.split_once('\t').unwrap_or(("normal", line));
        // Pad first, then style: padding is not applied around the colour codes.
        let label = format!("{priority:<8}");
        let mark = match priority {
            "high" => label.red().bold(),
            "low" => label.dark_grey().bold(),
            _ => label.cyan().bold(),
        };
        print_line(format!("{mark} {text}"));
        count += 1;
    }
    if count == 0 {
        print_status(Tone::Info, "INFO", "no notes yet");
    }
}

fn run(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    match &cli.cmd {
        Cmd::Add { priority, text } => {
            add(&cli.file, *priority, text)?;
            print_status(Tone::Success, "OK", &format!("added a {} note", priority.name()));
            if cli.verbose {
                print_line(format!("file: {}", cli.file.display()));
            }
        }
        Cmd::List { limit } => list(&cli.file, *limit),
        Cmd::Repl => repl(cli)?,
    }
    Ok(())
}

fn repl(cli: &Cli) -> std::io::Result<()> {
    print_box_banner("notes", "type `help` or `quit`");
    repl_run(repl_prompt("notes", "repl"), |line| {
        if matches!(line, "quit" | "exit") {
            return ReplControl::Exit;
        }
        let words = std::iter::once("notes").chain(line.split_whitespace());
        match Cli::try_parse_from(words) {
            Ok(Cli { cmd: Cmd::Repl, .. }) => print_error_line("already in the repl"),
            Ok(mut parsed) => {
                parsed.file.clone_from(&cli.file);
                if let Err(error) = run(&parsed) {
                    print_status(Tone::Error, "ERR", &error.to_string());
                }
            }
            // `help`, `--help` and `--version` are not failures: print them.
            Err(error) if error.is_display() => print!("{}", error.render()),
            Err(error) => print_error_line(error.render().trim_end()),
        }
        ReplControl::Continue(repl_prompt("notes", "repl"))
    })
}

fn main() {
    let cli = Cli::parse();
    if let Err(error) = run(&cli) {
        print_error_line(format!("error: {error}"));
        std::process::exit(1);
    }
}
```

## How it works

### 1. The command line is a type

`Cli` derives `Parser`; its fields are the global options and `cmd` holds the subcommand. Doc comments become help text, `value_enum` makes `--priority` accept only `low`, `normal`, `high`, and `global = true` lets `--file` and `-v` appear before or after the subcommand. `#[command(name = "notes", version = "0.1.0")]` turns on `-V/--version`; `visible_alias = "a"` makes `notes a ...` work and shows the alias in help. `Cli::parse()` reads the process arguments and, on `--help`, `--version` or a mistake, prints and exits (status 0 for help and version, 2 for a mistake).

Real output:

```text
$ notes --help
A tiny note keeper

Usage: notes [OPTIONS] <COMMAND>

Commands:
  add, a  Add a note
  list    Show the notes
  repl    Read commands from the terminal
  help    Print this message or the help of the given subcommand(s)

Options:
  -f, --file <PATH>  File that holds the notes [default: notes.txt]
  -v, --verbose      Print more detail
  -h, --help         Print help
  -V, --version      Print version

$ notes add
error: the following required arguments were not provided:
  <WORD>...

Usage: notes add [OPTIONS] <WORD>...

For more information, try '--help'.

$ notes add --priority urgent x
error: invalid value 'urgent' for '--priority <PRIORITY>'
  [possible values: low, normal, high]

Usage: notes add [OPTIONS] <WORD>...

For more information, try '--help'.
```

(Blank lines between sections are part of the real output. The `error:` text goes to standard error.)

### 2. Optional values

`--limit` has `num_args = 0..=1`, `require_equals = true` and `default_missing_value = "10"`: `notes list` shows everything, `notes list --limit=1` one note, `notes list --limit` ten. With `require_equals` the value is never taken from the next word, so `notes list --limit add` cannot swallow a subcommand name.

### 3. Output with quench-cli

`print_status(Tone::Success, "OK", "...")` writes a coloured label and a message to standard output; `print_error_line` (in `quench_cli::terminal`, not in the prelude) writes to standard error. Colour is applied with `Stylize` (`"high".red().bold()`). Setting `NO_COLOR` to anything non-empty removes every escape sequence, which is what the tests below do. Pad text **before** styling: a styled value ignores the width of the format string.

### 4. A REPL that reuses the parser

`repl_run(prompt, handler)` reads lines (with editing and history), trims them, skips empty ones, and calls the handler with each; `ReplControl::Continue(prompt)` keeps going, `Exit` stops, and Ctrl-C or Ctrl-D end the loop. The handler turns the line into words and runs the same parser: `Cli::try_parse_from(["notes", words...])` returns the error instead of exiting. Two kinds of `Err` need telling apart: `error.is_display()` is true for help and version (print `error.render()` as is), false for a mistake (print it on standard error). When standard input is not a terminal, the editor prints the prompt and reads plain lines, so the REPL also works in a pipe and in tests; at end of input it returns.

Real session (`NO_COLOR=1`, input piped):

```text
┌─────────────────────────────────────────────┐
│ notes                                       │
│ type `help` or `quit`                       │
└─────────────────────────────────────────────┘
notes(repl)> OK added a low note
notes(repl)> low      one two
notes(repl)> notes(repl)> A tiny note keeper
...
```

(`add` with no words printed the "required arguments" error on standard error; `repl` printed `already in the repl`; `nope` printed `error: unrecognized subcommand 'nope'`.)

### 5. Exit codes

`Parser::parse` exits 0/2 itself. For your own failures return from `main` yourself, as here (`std::process::exit(1)`), or return `Result<(), Box<dyn Error>>` from `main` (Rust prints the `Debug` form; `quench_cli::require::MissingBinary` prints one clean line in that case).

## Testing the tool

Parsing needs no process: `Cli::try_parse_from(["notes", "add", "--priority", "high", "x"])` returns the struct, and `Cli::command().debug_assert()` catches duplicate flags and clashes with `-h`/`-V`. To test the whole binary use `quench-test`'s command runner. The test target (`harness = false`) needs the `[[test]]` entry; `CARGO_BIN_EXE_notes` is set by Cargo for integration tests of the package, so `Command::cargo_bin("notes")` finds the binary:

```rust
use quench_test::cmd::Command;
use quench_test::predicate::prelude::*;

fn tmp(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("notes-{}-{name}.txt", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[quench_test::test]
fn add_and_list() {
    let file = tmp("a");
    let notes = |args: &[&str]| {
        let mut c = Command::cargo_bin("notes").unwrap();
        c.env("NO_COLOR", "1").args(["--file", file.to_str().unwrap()]).args(args.iter().copied());
        c.assert()
    };
    notes(&["list"]).success().stdout("INFO no notes yet\n");
    notes(&["add", "--priority", "high", "buy", "milk"]).success().stdout("OK added a high note\n");
    notes(&["-v", "add", "x"]).success().stdout(predicate::str::contains("file: "));
    notes(&["list", "--limit=1"]).success().stdout("high     buy milk\n");
}

#[quench_test::test]
fn mistakes_exit_with_2() {
    Command::cargo_bin("notes").unwrap().arg("add").assert().failure().code(2);
    Command::cargo_bin("notes").unwrap().arg("--version").assert().success().stdout("notes 0.1.0\n");
}

#[quench_test::test]
fn the_repl_works_from_a_pipe() {
    let file = tmp("r");
    Command::cargo_bin("notes").unwrap()
        .env("NO_COLOR", "1")
        .args(["--file", file.to_str().unwrap(), "repl"])
        .write_stdin("add --priority low one two\nlist\nquit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("low      one two"));
}

quench_test::main!();
```

`Command` runs the program with standard input closed (`/dev/null`) unless you call `write_stdin`, which is what a tool that reads a terminal needs in tests. When you run such tests by hand, close standard input too (`cargo test -p your-tool </dev/null`): the `quench-cli` and `quench-tool` test suites do, because an open terminal stdin makes their REPL tests wait for you.

## Checklist

* Call `Cli::command().debug_assert()` in a test.
* Use `try_parse_from` in tests; `parse()` exits the process.
* Print mistakes to standard error and results to standard output; let `Error::exit()` do it for parse errors.
* Honour `NO_COLOR` by using `quench-cli`'s helpers or `Stylize` rather than raw escape codes (the raw constants `BOLD`, `CYAN`, ... are not affected by it).
* Check external programs once with `require_binary("git", "git is needed to ...")?` before starting work.
* Only the attributes listed in [quench-args-macros](../crates/quench-args-macros/attributes.md) exist; anything else is a compile error that lists what is supported.
