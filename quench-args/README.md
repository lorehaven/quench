# quench-args
Command-line argument parsing with derive macros, no dependencies beyond its own macros.

## What it is
`quench-args` parses command lines and replaces `clap` for the tools of the workspace. The derives `Parser`, `Args`, `Subcommand` and `ValueEnum` use clap's attribute names (`#[command(..)]`, `#[arg(..)]`, `#[value(..)]`), so a tool switches by changing its `use`. The builder types `Command`, `Arg` and `Matches` are public for tools that do not use the derives.

The derives come from `quench-args-macros` and are re-exported here. The runtime library has no third-party dependency; it pairs with `quench-cli` for building command-line tools.

## Use cases
- Define a tool's flags, options and positionals as a struct.
- Build nested subcommands with aliases and global options.
- Parse a REPL line with `try_parse_from`.
- Generate `--help` and `--version` output from doc comments.

## Example
```rust
use quench_args::Parser;

#[derive(Parser, Debug)]
#[command(name = "copy", version = "1.0.0", about = "Copy things")]
struct Cli {
    /// Overwrite existing files.
    #[arg(short, long)]
    force: bool,
    /// How many times to retry.
    #[arg(long, default_value_t = 3)]
    retries: u32,
    /// Where to write.
    #[arg(long, short = 'o', value_name = "DIR")]
    out: Option<String>,
    /// Sources.
    #[arg(required = true)]
    sources: Vec<String>,
}

let cli = Cli::try_parse_from(["copy", "-f", "--retries=5", "a", "b"]).unwrap();
assert!(cli.force && cli.retries == 5 && cli.out.is_none());
assert_eq!(cli.sources, ["a", "b"]);
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-args/
