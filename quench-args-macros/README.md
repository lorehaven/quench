# quench-args-macros
Derive macros for quench-args: Parser, Args, Subcommand and ValueEnum.

## What it is
`quench-args-macros` is the proc-macro crate behind `quench-args`. It provides the four derives `Parser`, `Args`, `Subcommand` and `ValueEnum` and the helper attributes `#[command(..)]`, `#[arg(..)]` and `#[value(..)]`, named as clap names them. Its dependencies (`proc-macro2`, `quote`, `syn`) are used at compile time only.

You normally depend on `quench-args` only, which re-exports the derives. The generated code refers to `::quench_args::..`, so the crate that derives needs `quench-args` as a dependency.

## Use cases
- Turn a struct into a command-line parser with `Parser`.
- Share a group of arguments between commands with `Args` and `#[command(flatten)]`.
- Define subcommands as an enum with `Subcommand`.
- Accept a fixed set of values with `ValueEnum`.

## Example
```rust
use quench_args::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "riv")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// List the resources
    #[command(visible_alias = "ls")]
    List { targets: Vec<String> },
    #[command(name = "renamed")]
    Original,
}

let cli = Cli::try_parse_from(["riv", "ls", "x"]).unwrap();
assert!(matches!(cli.cmd, Some(Cmd::List { .. })));
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-args-macros/
