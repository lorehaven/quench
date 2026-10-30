---
title: quench-args-macros
parent: Crates
has_children: true
permalink: /crates/quench-args-macros/
nav_order: 5
---

`quench-args-macros` is the proc-macro crate behind [quench-args](../quench-args/index.md). It provides four derives, `Parser`, `Args`, `Subcommand` and `ValueEnum`, and the helper attributes `#[command(..)]`, `#[arg(..)]` and `#[value(..)]`, named as clap names them. Use it through `quench-args`, which re-exports the derives; the generated code refers to `::quench_args::..` and so needs `quench-args` as a dependency of the crate that derives.

## Facts

| | |
|---|---|
| Version | 0.1.0 |
| Edition | 2024 |
| Kind | `[lib] proc-macro = true` |
| Dependencies | `proc-macro2`, `quote`, `syn` (compile time only) |
| Features | none |
| Public items | 4 derive macros |

```toml
# you normally depend on quench-args only
[dependencies]
quench-args = { version = "0.1.2", registry = "ennor" }
```

## Pages

* [Derives and attributes](attributes.md): every derive, every attribute and argument, doc-comment handling, naming rules.
* [Generated code and errors](generated.md): what each derive implements and every compile error.

## Minimal example

```rust
use quench_args::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "tool", version)]
struct Cli {
    #[arg(long, value_enum, default_value_t = Mode::Fast)]
    mode: Mode,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Say hello
    Hello,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq)]
enum Mode { Fast, SlowMode }

let cli = Cli::try_parse_from(["tool", "--mode", "slow-mode", "hello"]).unwrap();
assert_eq!(cli.mode, Mode::SlowMode);
```
