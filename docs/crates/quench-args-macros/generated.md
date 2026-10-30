---
title: Generated code and errors
parent: quench-args-macros
grand_parent: Crates
nav_order: 2
---

## What `#[derive(Parser)]` writes

For `struct Cli`, roughly:

```rust
impl ::quench_args::Args for Cli {
    fn augment_args(cmd: ::quench_args::Command) -> ::quench_args::Command { /* one Arg per field */ }
}
impl ::quench_args::FromMatches for Cli {
    fn from_matches(m: &::quench_args::Matches) -> Result<Self, ::quench_args::Error> { /* one expression per field */ }
}
impl ::quench_args::CommandFactory for Cli {
    fn command() -> ::quench_args::Command {
        let cmd = ::quench_args::Command::new(/* name */);
        /* about, long_about, version, help_template, aliases, hide */
        <Self as ::quench_args::Args>::augment_args(cmd)
    }
}
impl ::quench_args::Parser for Cli {}
```

`Args` writes the first two; `Subcommand` writes `Subcommand::augment_subcommands` (one `Command` per variant added through `cmd.subcommand(..)`) and a `FromMatches` that matches `m.subcommand()` by variant name; `ValueEnum` writes `value_variants` and `to_possible_value`.

Per field of a struct: `Arg::new(id)`, then `.long`, `.short`, `.value_name`, `.help`, `.long_help`, `.kind(Kind::..)`, `.required(true)` (when required), `.global(true)`, `.default_value(..)`, `.conflicts_with(..)`, `.optional_value(..)`, `.require_equals(true)`, `.trailing_var_arg(true)`, `.allow_hyphen_values(true)`, `.hide(true)`, `.possible_values(possible_values::<T>())`, in that order. The error name for a parse failure is `--long <PLACEHOLDER>`, `-s <PLACEHOLDER>` or `<PLACEHOLDER>` (computed at compile time; a non-literal `value_name` falls back to the upper-cased id there).

## Compile errors

All are `syn::Error`s, shown by rustc at the attribute or item. Exact texts:

| Cause | Message |
|---|---|
| unknown key in `#[command(..)]` | `` `key` is not supported in `#[command(..)]`; supported: `name`, `version`, `about`, `long_about`, `help_template`, `disable_help_subcommand`, `alias`, `aliases`, `visible_alias`, `visible_aliases`, `hide`, `subcommand`, `flatten` `` |
| unknown key in `#[arg(..)]` | `` `key` is not supported in `#[arg(..)]`; supported: `long`, `short`, `value_name`, `help`, `long_help`, `default_value`, `default_value_t`, `default_missing_value`, `num_args = 0..=1`, `require_equals`, `value_enum`, `global`, `required`, `conflicts_with`, `trailing_var_arg`, `allow_hyphen_values`, `hide` `` |
| unknown key in `#[value(..)]` | `` `key` is not supported in `#[value(..)]`; supported: `name`, `help`, `skip` `` |
| `num_args` other than `0..=1` | `only num_args = 0..=1 (an optional value) is supported` |
| `num_args = 0..=1` without `default_missing_value` | `` `num_args = 0..=1` needs a `default_missing_value` (what the option means when given no value) `` |
| `version` on a variant | `` `version` only applies to the top-level command `` |
| `Parser`/`Args` on an enum | `` `Parser` and `Args` derive on structs; use `Subcommand` for an enum `` |
| `Subcommand` on a struct | `` `Subcommand` derives on enums `` |
| `ValueEnum` on a struct | `` `ValueEnum` derives on enums `` |
| `ValueEnum` variant with data | `` `ValueEnum` variants carry no data `` |
| tuple struct | `tuple fields are not supported here; use named fields` |
| `bool` positional | `` a `bool` field is a switch: give it `long` or `short` `` |
| tuple variant with 0 or 2+ fields | `` a tuple variant takes exactly one field: an `Args` struct (or, with `#[command(subcommand)]`, a `Subcommand` enum) `` |
| `long` with a non-string, `short` with a non-char, `conflicts_with` with a non-string, `#[value(name/help)]` with a non-string | the `syn` parse error for the expected literal |

Unit structs derive fine (a command with no arguments).

## Using the derives in tests

`quench-args-macros/src/tests.rs` (plain `#[test]`, default harness) calls the `expand_*` functions on `syn::parse_quote!` inputs and compares the token text; use the same pattern for new attributes. See [testing](../quench-args/testing.md) for the behaviour tests in `quench-args`.
