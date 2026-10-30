---
title: quench-args
parent: Crates
has_children: true
permalink: /crates/quench-args/
nav_order: 4
---

`quench-args` (library `quench_args`) parses command lines. It replaces `clap` for the tools of the workspace: the derives `Parser`, `Args`, `Subcommand` and `ValueEnum` use clap's attribute names (`#[command(..)]`, `#[arg(..)]`, `#[value(..)]`), so a tool switches by changing its `use`. The same builder types (`Command`, `Arg`, `Matches`) are public for tools that do not use the derives.

## Facts

| | |
|---|---|
| Version | 0.1.2 |
| Edition | 2024 (workspace; no `rust-version` is declared; the sources use let-chains, so a toolchain that supports edition 2024 is needed) |
| License | MIT |
| Test harness | `harness = false` in `[lib]`; the unit tests run through `quench_test::main!()` (see [testing](testing.md)) |
| Unsafe | none (`unsafe_code = "deny"` in the workspace lints) |

## Install

```toml
[dependencies]
quench-args = { version = "0.1.2", registry = "ennor" }
```

Inside the workspace: `quench-args = { workspace = true }`.

## Features

None. `Cargo.toml` has no `[features]` table.

## Dependencies

| Crate | Kind | Why |
|---|---|---|
| `quench-args-macros` | normal | the four derives, re-exported from this crate (see [quench-args-macros](../quench-args-macros/index.md)) |
| `quench-test` | dev | test harness for the unit tests |

`quench-args-macros` itself uses `syn`, `quote` and `proc-macro2` at compile time only. The runtime library has no third-party dependency.

## Where to go next

* [Usage](usage.md): worked examples from a single struct to nested subcommands, optional values and hand-built commands.
* [API reference](api.md): every public item with its signature.
* [Errors](errors.md): every `ErrorKind`, the message text and when it occurs.
* [Internals](internals.md): the parsing rules, help layout and the differences from clap.
* [Testing](testing.md)
* The derives and their attributes: [quench-args-macros](../quench-args-macros/index.md).
* A complete tool built with `quench-args` and `quench-cli`: [Building command-line tools](../../guides/building-cli-tools.md).

## Minimal example

This is the example from the crate documentation (a doc test).

```rust
use quench_args::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "tool", version)]
struct Cli {
    /// Environment to act on.
    #[arg(long, short = 'e', global = true, value_name = "ENV")]
    env: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Render the manifests
    Render {
        #[arg(long, value_enum, default_value_t = Scope::All)]
        scope: Scope,
        targets: Vec<String>,
    },
    /// Say hello
    #[command(visible_alias = "hi")]
    Hello,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
enum Scope { All, Mutable }

let cli = Cli::try_parse_from(["tool", "render", "--scope", "mutable", "a/b"]).unwrap();
assert!(matches!(cli.cmd, Cmd::Render { scope: Scope::Mutable, .. }));
```

In a program, call `Cli::parse()`: it reads `std::env::args_os()` and, on `--help`, `--version` or a mistake, prints and exits (status 0 for help and version, 2 for a mistake).
