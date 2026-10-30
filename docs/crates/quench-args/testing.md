---
title: Testing
parent: quench-args
grand_parent: Crates
nav_order: 5
---

## How the crate is tested

`quench-args` sets `harness = false` for its library tests and ends `src/lib.rs` with

```rust
#[cfg(test)]
mod tests;

#[cfg(test)]
quench_test::main!();
```

so `cargo test -p quench-args` runs the `quench-test` harness (see [quench-test](../quench-test/index.md)). The tests are in `src/tests.rs` (34 `#[quench_test::test]` functions) and cover: global options before and after subcommands, nested subcommands and aliases, value enums (default, validation, case), clusters, defaults, trailing var args, `--`, required and unknown arguments, conflicts, tuple variants, help/version output, the `help` subcommand, `help_for`, introspection, help templates, error rendering, non-UTF-8 input, direct `Matches` use, every `debug_assert` panic, possible values and hidden flags, optional values. The test definitions (`Cli`, `Anvil`) are good models for your own tests.

The doc test in `src/lib.rs` is the example on the [overview](index.md).

## Testing a tool that uses it

* Use `Cli::try_parse_from(["prog", ...])` in tests; it never exits.
* Assert on `Error::kind()` and `render()`; call `Cli::command().debug_assert()` once to catch duplicate flags, clashes with `-h`/`-V` and unknown `conflicts_with` ids.
* For help text, `Cli::command().render_help()` and `help_for(&["sub"], true)`.
