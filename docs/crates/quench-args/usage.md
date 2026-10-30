---
title: Usage
parent: quench-args
grand_parent: Crates
nav_order: 1
---

Worked examples, from the common to the advanced. The tests behind them are in `quench-args/src/tests.rs`.

## 1. A struct with flags, options and positionals

A field's type decides its shape:

| Field type | Shape |
|---|---|
| `bool` | a switch (needs `long` or `short`, otherwise a compile error) |
| `Option<T>` | an optional value |
| `Vec<T>` | any number of values: a repeatable option when it has `long`/`short`, else the rest of the positionals |
| any other `T` | required, unless the attribute gives `default_value` / `default_value_t` or `required = false` |

A field with neither `long` nor `short` is a positional. `T` is any `FromStr` type whose `Err` is `Display`.

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

Effect: `copy --help` prints the `Usage:`, `Arguments:` and `Options:` tables, with `[default: 3]` appended to `--retries`. Pitfalls:

* `-f` comes from `short` with no value: it takes the first character of the field name.
* A `Vec<T>` positional without `required = true` accepts zero values.
* `try_parse_from` takes the program name as the first item, as `std::env::args_os()` yields it.

## 2. Subcommands

A `Subcommand` enum is attached to a struct with `#[command(subcommand)]`. An `Option<Enum>` field makes the subcommand optional; a plain `Enum` field makes it required (`MissingSubcommand`).

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
assert!(Cli::try_parse_from(["riv"]).unwrap().cmd.is_none());
```

Variant names become kebab-case subcommand names (`SetBranch` is `set-branch`); `#[command(name = "..")]` overrides. Subcommand `-h` lists `list, ls` with the alias after the name. A `help` subcommand is added to every command that has subcommands unless `disable_help_subcommand = true` or the command defines its own `help`.

## 3. Global options

`#[arg(global = true)]` makes an option accepted after any subcommand too; its value is kept at the level that declares it.

```rust
# use quench_args::{Parser, Subcommand};
#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, short = 'e', global = true)]
    env: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand, Debug)]
enum Cmd { Apply, Plan }

let a = Cli::try_parse_from(["t", "-e", "prod", "apply"]).unwrap();
let b = Cli::try_parse_from(["t", "apply", "-e", "prod"]).unwrap();
assert_eq!(a.env, b.env);
```

Giving the option at two levels is allowed (the deepest wins); giving it twice at one level is an `ArgumentConflict` error ("cannot be used multiple times").

## 4. Value enums

```rust
use quench_args::{Parser, ValueEnum};

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    All,
    /// Only the mutable ones
    Mutable,
    #[value(name = "imm")]
    Immutable,
    #[value(skip)]
    Hidden,
}

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, value_enum, default_value_t = Scope::All)]
    scope: Scope,
}

assert_eq!(Cli::try_parse_from(["t", "--scope", "imm"]).unwrap().scope, Scope::Immutable);
assert!(Cli::try_parse_from(["t", "--scope", "hidden"]).is_err());
```

Help shows `[default: all]` and `[possible values: all, mutable, imm]`. A wrong spelling fails with `invalid value 'x' for '--scope <SCOPE>'` and a `[possible values: ..]` line. Matching is case-sensitive; `ValueEnum::from_str(input, true)` ignores case but the derive never calls it with `true`.

## 5. Optional values (`--tail` or `--tail=200`)

```rust
use quench_args::Parser;

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, value_name = "N", num_args = 0..=1, require_equals = true, default_missing_value = "80")]
    tail: Option<usize>,
    #[arg(short = 'p', long, num_args = 0..=1, default_missing_value = "all")]
    pick: Option<String>,
}

assert_eq!(Cli::try_parse_from(["t", "--tail"]).unwrap().tail, Some(80));
assert_eq!(Cli::try_parse_from(["t", "--tail=200"]).unwrap().tail, Some(200));
assert_eq!(Cli::try_parse_from(["t", "--pick", "x"]).unwrap().pick.as_deref(), Some("x"));
assert_eq!(Cli::try_parse_from(["t", "-p"]).unwrap().pick.as_deref(), Some("all"));
```

Without `require_equals`, the next word is taken as the value unless it looks like a flag (starts with `-` and is longer than one character). `num_args = 0..=1` needs `default_missing_value`, otherwise a compile error. Help prints `--tail[=<N>]` when `require_equals` is set and `--pick [<PICK>]` otherwise.

## 6. Pass-through arguments

```rust
use quench_args::{Parser, Subcommand};

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}
#[derive(Subcommand, Debug)]
enum Cmd {
    /// Run a program with its own flags
    Test {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

let cli = Cli::try_parse_from(["t", "test", "a", "--nocapture", "-x"]).unwrap();
let Cmd::Test { args } = cli.cmd;
assert_eq!(args, ["a", "--nocapture", "-x"]);
```

`trailing_var_arg` makes everything after the first value of that positional a value, flags included. `--` also ends option parsing for the rest of the line.

## 7. Reusable groups and tuple variants

```rust
use quench_args::{Args, Parser, Subcommand};

#[derive(Args, Debug)]
struct BranchArgs {
    /// Owner and name
    repo: String,
    branch: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    SetBranch(BranchArgs),
}

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}
```

A tuple variant takes exactly one field, an `Args` struct (or, with `#[command(subcommand)]` on the variant, a nested `Subcommand` enum). In a struct, `#[command(flatten)]` on a field of an `Args` type adds that group's arguments to the command.

## 8. Handling errors yourself

`Cli::parse()` exits. To control the output, use `try_parse` or `try_parse_from` and look at `Error::kind()`:

```rust
# use quench_args::{Parser, ErrorKind};
# #[derive(Parser, Debug)] struct Cli { #[arg(long)] n: u32 }
match Cli::try_parse_from(["t", "--n", "x"]) {
    Ok(_) => {}
    Err(e) if e.is_display() => print!("{}", e.render()),   // --help / --version
    Err(e) => {
        assert_eq!(e.kind(), ErrorKind::InvalidValue);
        eprint!("{}", e.render());   // "error: invalid value 'x' for '--n <N>': ..."
    }
}
```

## 9. Without the derives

```rust
use quench_args::{Arg, Command, Kind};

let command = Command::new("m")
    .version("1.0")
    .arg(Arg::new("verbose").short('v').long("verbose").kind(Kind::Flag))
    .arg(Arg::new("file").required(true))
    .subcommand(Command::new("run").about("Run it"));
command.debug_assert();
let m = command.try_get_matches_from(["-v", "a.txt"]).unwrap();   // no program name here
assert!(m.flag("verbose"));
assert_eq!(m.get_one("file"), Some("a.txt"));
```

Note the difference: `Command::try_get_matches_from` takes the arguments **without** the program name; `Parser::try_parse_from` takes them **with** it (it skips the first).

## 10. Help for a subcommand path

`Command::help_for(&["env", "set"], false)` returns the help of `tool env set` with the usage line `Usage: tool env set [OPTIONS] <ENV>` and the ancestors' global options. It returns `None` when a name is not a subcommand. A `quench-cli` REPL or a custom `help` handler can use it.
