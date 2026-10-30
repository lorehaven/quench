---
title: API reference
parent: quench-args
grand_parent: Crates
nav_order: 2
---

Every public item of `quench_args`, in source order (`lib.rs`, then `command.rs`, `matches.rs`, `error.rs`). Signatures are copied from the code. Nothing here is `unsafe`; no item is feature-gated.

## Re-exports (`lib.rs`)

```rust
pub use command::{Arg, Command, Kind, PossibleValue};
pub use error::{Error, ErrorKind};
pub use matches::Matches;
pub use quench_args_macros::{Args, Parser, Subcommand, ValueEnum};
```

`Args`, `Parser`, `Subcommand` and `ValueEnum` are **both** derive macros and (same names) traits defined in this crate; `use quench_args::Parser;` brings both. The derives are described in [quench-args-macros](../quench-args-macros/index.md). The generated code names everything as `::quench_args::..` (the crate contains `extern crate self as quench_args;` so its own tests can derive too).

## Traits and functions (`lib.rs`)

### `trait FromArg`

```rust
pub trait FromArg: Sized {
    fn from_arg(arg: &str, raw: &str) -> Result<Self, Error>;
}

impl<T: FromStr> FromArg for T where T::Err: Display { .. }
```

Reads a typed value from the text of one argument. `arg` is the display name used in the error (`--n <N>`), `raw` the text. The blanket impl covers every `FromStr` type whose error is `Display` (`String`, `PathBuf`, integers, floats, `bool`, your own) and fails with `Error::invalid_value(arg, raw, &err.to_string())`, kind `ErrorKind::InvalidValue`. Because of the blanket impl you cannot implement `FromArg` for a type that is `FromStr`; implement `FromStr` instead.

### `trait ValueEnum`

```rust
pub trait ValueEnum: Sized + Clone + 'static {
    fn value_variants() -> &'static [Self];
    fn to_possible_value(&self) -> Option<PossibleValue>;
    fn from_str(input: &str, ignore_case: bool) -> Result<Self, String> { .. }
}
```

An enum whose variants are the accepted spellings of a value. `value_variants` lists the variants that can be named (variants marked `#[value(skip)]` are left out), `to_possible_value` gives the spelling and help of one (`None` hides it). The provided `from_str` finds the variant whose `PossibleValue::get_name` equals `input` (`eq_ignore_ascii_case` when `ignore_case`) and otherwise returns `Err("invalid variant: {input}")`. Also the name of the derive.

### `fn value_enum_arg`

```rust
pub fn value_enum_arg<T: ValueEnum>(arg: &str, raw: &str) -> Result<T, Error>
```

Used by the derive for `value_enum` fields: `T::from_str(raw, false)`, the error mapped to `Error::invalid_value(arg, raw, reason)`.

### `fn possible_values`

```rust
#[must_use]
pub fn possible_values<T: ValueEnum>() -> Vec<PossibleValue>
```

The `PossibleValue` of every variant in `T::value_variants()` that has one. The derive passes it to `Arg::possible_values`.

### `trait FromMatches`

```rust
pub trait FromMatches: Sized {
    fn from_matches(matches: &Matches) -> Result<Self, Error>;
}
```

Builds a type from what was matched at one command level. The derives implement it; `Error` kinds: `InvalidValue` when a value does not parse, `MissingRequired` / `MissingSubcommand` when the matches lack what the type needs.

### `trait CommandFactory`

```rust
pub trait CommandFactory {
    fn command() -> Command;
}
```

Implemented by `#[derive(Parser)]` only. `Cli::command()` builds the `Command` tree (name from `#[command(name)]` or `CARGO_PKG_NAME` of the crate using the derive).

### `trait Args`

```rust
pub trait Args: FromMatches {
    fn augment_args(cmd: Command) -> Command;
}
```

A group of arguments that can be added to a command. Implemented by `#[derive(Args)]` and by `#[derive(Parser)]` (a parser is also an `Args`).

### `trait Subcommand`

```rust
pub trait Subcommand: FromMatches {
    fn augment_subcommands(cmd: Command) -> Command;
}
```

An enum of subcommands. Implemented by `#[derive(Subcommand)]`.

### `trait Parser`

```rust
pub trait Parser: CommandFactory + FromMatches {
    #[must_use] fn parse() -> Self { .. }
    fn try_parse() -> Result<Self, Error> { .. }
    #[must_use] fn parse_from<I, T>(args: I) -> Self
        where I: IntoIterator<Item = T>, T: Into<OsString> + Clone { .. }
    fn try_parse_from<I, T>(args: I) -> Result<Self, Error>
        where I: IntoIterator<Item = T>, T: Into<OsString> + Clone { .. }
}
```

A type read from the command line. All four methods have default bodies; `#[derive(Parser)]` writes an empty `impl Parser`.

| Method | Behaviour |
|---|---|
| `parse()` | `parse_from(std::env::args_os())`. On help, version or a mistake it prints and calls `std::process::exit` (see `Error::exit`). |
| `try_parse()` | `try_parse_from(std::env::args_os())`. |
| `parse_from(args)` | `try_parse_from(args)`, calling `e.exit()` on `Err`. The first item is the program name. |
| `try_parse_from(args)` | Builds `Self::command()`, skips the first item of `args`, parses the rest with `Command::try_get_matches_from`, then `from_matches`. |

Errors: as `Command::try_get_matches_from`, plus the `FromMatches` errors.

## `command.rs`

### `struct PossibleValue`

`#[derive(Debug, Clone, PartialEq, Eq)]`. One accepted spelling of an enumerated value.

```rust
impl PossibleValue {
    #[must_use] pub fn new(name: impl Into<String>) -> Self
    #[must_use] pub fn help(mut self, help: impl Into<String>) -> Self
    #[must_use] pub const fn hide(mut self, hide: bool) -> Self
    #[must_use] pub fn get_name(&self) -> &str
    #[must_use] pub fn get_help(&self) -> Option<&str>
    #[must_use] pub const fn is_hide_set(&self) -> bool
}
```

`new` creates a visible value without help. `hide(true)` keeps the value accepted but leaves it out of `[possible values: ..]` in help and in the invalid-value message.

### `enum Kind`

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`.

| Variant | Meaning |
|---|---|
| `Flag` | a switch with no value: present or not |
| `Single` | one value (`--name value`, or one positional); a repeat replaces the earlier one |
| `Many` | any number of values: a positional that takes the rest, or an option that may repeat |

### `struct Arg`

`#[derive(Debug, Clone)]`. One flag, option or positional. All fields are private; build it with the methods below (each takes and returns `self`).

```rust
impl Arg {
    #[must_use] pub fn new(id: impl Into<String>) -> Self
    #[must_use] pub fn long(mut self, long: impl Into<String>) -> Self
    #[must_use] pub const fn short(mut self, short: char) -> Self
    #[must_use] pub fn value_name(mut self, name: impl Into<String>) -> Self
    #[must_use] pub fn help(mut self, help: impl Into<String>) -> Self
    #[must_use] pub fn long_help(mut self, help: impl Into<String>) -> Self
    #[must_use] pub const fn kind(mut self, kind: Kind) -> Self
    #[must_use] pub const fn required(mut self, required: bool) -> Self
    #[must_use] pub const fn global(mut self, global: bool) -> Self
    #[must_use] pub fn default_value(mut self, value: impl Into<String>) -> Self
    #[must_use] pub fn conflicts_with(mut self, id: impl Into<String>) -> Self
    #[must_use] pub const fn trailing_var_arg(mut self, on: bool) -> Self
    #[must_use] pub const fn allow_hyphen_values(mut self, on: bool) -> Self
    #[must_use] pub const fn hide(mut self, hide: bool) -> Self
    #[must_use] pub fn optional_value(mut self, missing: impl Into<String>) -> Self
    #[must_use] pub const fn require_equals(mut self, on: bool) -> Self
    #[must_use] pub fn possible_values(mut self, values: impl IntoIterator<Item = PossibleValue>) -> Self
    #[must_use] pub fn get_id(&self) -> &str
    #[must_use] pub fn get_long(&self) -> Option<&str>
    #[must_use] pub const fn get_short(&self) -> Option<char>
    #[must_use] pub const fn is_positional(&self) -> bool
    #[must_use] pub const fn is_required_set(&self) -> bool
    #[must_use] pub const fn is_global_set(&self) -> bool
}
```

Defaults after `Arg::new(id)`: no long, no short, `Kind::Single`, not required, not global, no default, no conflicts, not trailing, hyphen values not allowed, not hidden, no possible values, no optional value, `require_equals` off. `new` makes a **positional**; adding `long` or `short` makes it an option. `Kind::Flag` needs a `long` or `short` (`Command::debug_assert` checks it).

| Method | Effect |
|---|---|
| `new(id)` | the id is the key used with `Matches` (`flag`, `get_one`, ...) and in `conflicts_with`. |
| `long(name)` | `--name`. |
| `short(c)` | `-c`. `-h` and (with a version) `-V` are taken by help and version. |
| `value_name(name)` | the placeholder shown for the value (`--env <ENV>`). Default: the id in upper case, `-` replaced by `_`. |
| `help(text)` | one-line help shown by `-h`. |
| `long_help(text)` | fuller help shown by `--help`; falls back to `help`. |
| `kind(kind)` | see `Kind`. |
| `required(bool)` | parsing fails with `MissingRequired` when it was neither typed nor defaulted. |
| `global(bool)` | accepted after any subcommand too; value kept at the declaring level. |
| `default_value(text)` | value used when none is given, as the text it would be typed as. For a `Flag`, the defaults `"true"`, `"1"`, `"yes"`, `"on"` switch the flag on when it is not typed; any other text leaves it off. Help shows `[default: ..]` for non-flags only. |
| `conflicts_with(id)` | may be called repeatedly; cannot be typed together with argument `id` (see [internals](internals.md#conflicts)). |
| `trailing_var_arg(bool)` | for a positional: once its first value is seen, every later word is a value, flags included. |
| `allow_hyphen_values(bool)` | a value may start with `-`. For an option it lets the next word be taken as its value even if it looks like a flag; for a positional it lets unknown `-x`/`--x` words be values. |
| `hide(bool)` | left out of help (arguments table and usage). |
| `optional_value(missing)` | the option's value becomes optional: `--name` alone means `missing`; `--name=v` and (unless `require_equals`) `--name v` give a value. |
| `require_equals(bool)` | with `optional_value`: the value must be attached with `=`; the next word is never taken. |
| `possible_values(iter)` | only these spellings are accepted (exact, case-sensitive); the invalid-value error lists the visible ones. |

Getters: `get_id`, `get_long`, `get_short`, `is_positional` (no long and no short), `is_required_set`, `is_global_set`.

### `struct Command`

`#[derive(Debug, Clone)]`. A command and the arguments and subcommands it accepts. Fields are private.

```rust
impl Command {
    #[must_use] pub fn new(name: impl Into<String>) -> Self
    #[must_use] pub fn name(mut self, name: impl Into<String>) -> Self
    #[must_use] pub fn about(mut self, about: impl Into<String>) -> Self
    #[must_use] pub fn long_about(mut self, about: impl Into<String>) -> Self
    #[must_use] pub fn version(mut self, version: impl Into<String>) -> Self
    #[must_use] pub fn arg(mut self, arg: Arg) -> Self
    #[must_use] pub fn subcommand(mut self, command: Self) -> Self
    #[must_use] pub fn alias(mut self, alias: impl Into<String>) -> Self
    #[must_use] pub fn visible_alias(mut self, alias: impl Into<String>) -> Self
    #[must_use] pub fn help_template(mut self, template: impl Into<String>) -> Self
    #[must_use] pub const fn disable_help_subcommand(mut self, disable: bool) -> Self
    #[must_use] pub const fn subcommand_required(mut self, required: bool) -> Self
    #[must_use] pub const fn hide(mut self, hide: bool) -> Self
    #[must_use] pub fn get_name(&self) -> &str
    #[must_use] pub fn get_about(&self) -> Option<&str>
    #[must_use] pub fn get_version(&self) -> Option<&str>
    pub fn get_subcommands(&self) -> impl Iterator<Item = &Self>
    pub fn get_arguments(&self) -> impl Iterator<Item = &Arg>
    pub fn get_visible_aliases(&self) -> impl Iterator<Item = &str>
    pub fn get_all_aliases(&self) -> impl Iterator<Item = &str>
    #[must_use] pub fn find_subcommand(&self, name: &str) -> Option<&Self>
    pub fn debug_assert(&self)
}
```

| Method | Effect |
|---|---|
| `new(name)` / `name(name)` | the command name (the program name for the root; shown in usage and the version line). |
| `about(text)` | one-line description shown in a parent's command list and by `-h`. |
| `long_about(text)` | fuller description shown by `--help`; falls back to `about`. |
| `version(text)` | turns on `-V` / `--version`, which print `<root name> <version>\n`. Without a version, `-V`/`--version` are ordinary unknown arguments. |
| `arg(arg)` | adds an argument. Order is the order in help and the order positionals are filled. |
| `subcommand(cmd)` | adds a subcommand. |
| `alias(name)` | an extra name that works but is not listed. |
| `visible_alias(name)` | an extra name that works and is listed after the name in the commands table. |
| `help_template(t)` | replaces the default help layout; see [internals](internals.md#help-templates). |
| `disable_help_subcommand(true)` | does not add the implicit `help` subcommand. |
| `subcommand_required(true)` | `MissingSubcommand` when none is given (only if the command has subcommands). |
| `hide(true)` | left out of its parent's command list. |
| `find_subcommand(name)` | by name or any alias. |
| `get_all_aliases()` | hidden aliases first, then visible ones. |

`debug_assert` panics (message names the command and the problem) on: a duplicate argument id; a duplicate `--long` or `-s`; `--help`/`-h` (and `--version`/`-V` when a version is set) declared as arguments; a `Flag` with no long or short; `conflicts_with` naming an unknown id; a duplicate subcommand name or an alias used twice; recursively for every subcommand. Derive-based programs should call `Cli::command().debug_assert()` in a test.

### Methods added in `lib.rs`

```rust
impl Command {
    pub fn try_get_matches_from<I, T>(&self, args: I) -> Result<Matches, Error>
        where I: IntoIterator<Item = T>, T: Into<OsString>
    #[must_use] pub fn render_help(&self) -> String
    #[must_use] pub fn render_long_help(&self) -> String
    pub fn print_help(&self) -> std::io::Result<()>
    #[must_use] pub fn help_for(&self, path: &[&str], long: bool) -> Option<String>
}
```

* `try_get_matches_from(args)`: parses `args` **without** the program name. A word that is not valid UTF-8 gives `ErrorKind::InvalidUtf8` ("invalid UTF-8 was detected in one or more arguments: ..."). Otherwise the first mistake found, or `DisplayHelp` / `DisplayVersion` when the user asked for those (an `Err` whose `is_display()` is true).
* `render_help()` is the text `-h` prints; `render_long_help()` the text `--help` prints.
* `print_help()` writes `render_help()` to standard output and flushes; returns the I/O error if it cannot.
* `help_for(path, long)`: help of the subcommand reached through `path` (names or aliases, outermost first), usage showing the whole path, and the ancestors' global options. `None` if a name is not a subcommand. `help_for(&[], false)` equals `render_help()`.

## `matches.rs`

### `struct Matches`

`#[derive(Debug, Clone, Default)]`. The values of one command's arguments, and the subcommand chosen (with its own matches). Fields are private.

```rust
impl Matches {
    #[must_use] pub fn flag(&self, id: &str) -> bool
    #[must_use] pub fn is_present(&self, id: &str) -> bool
    #[must_use] pub fn get_one(&self, id: &str) -> Option<&str>
    #[must_use] pub fn get_many(&self, id: &str) -> &[String]
    #[must_use] pub fn subcommand(&self) -> Option<(&str, &Self)>
    pub fn get_required(&self, id: &str) -> Result<&str, crate::Error>
}
```

| Method | Returns |
|---|---|
| `flag(id)` | whether the switch `id` was given (or defaulted on). |
| `is_present(id)` | whether the user **typed** `id`; a default does not count. |
| `get_one(id)` | the value; when repeated, the last; `None` if absent. Defaults count. |
| `get_many(id)` | every value in the order given; empty slice if none. A default counts as one value. |
| `subcommand()` | `(canonical name, matches)` of the subcommand chosen. The name is the subcommand's own name, never the alias that was typed. |
| `get_required(id)` | `get_one`, or `Err` of kind `MissingRequired`: "the argument '<id>' was not provided". |

Values of a `global` argument are kept in the `Matches` of the command that declares it, wherever it was typed.

## `error.rs`

### `enum ErrorKind`

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. Variants: `DisplayHelp`, `DisplayVersion`, `UnknownArgument`, `UnknownSubcommand`, `MissingRequired`, `MissingSubcommand`, `MissingValue`, `InvalidValue`, `ArgumentConflict`, `UnexpectedValue`, `InvalidUtf8`. Meaning and message texts: [errors](errors.md).

### `struct Error`

`#[derive(Debug, Clone)]`; implements `std::fmt::Display` and `std::error::Error`. Fields are private.

```rust
impl Error {
    #[must_use] pub fn invalid_value(arg: &str, value: &str, reason: &str) -> Self
    #[must_use] pub const fn kind(&self) -> ErrorKind
    #[must_use] pub const fn is_display(&self) -> bool
    #[must_use] pub fn message(&self) -> &str
    #[must_use] pub fn render(&self) -> String
    pub fn print(&self)
    pub fn exit(&self) -> !
    #[must_use] pub fn missing_subcommand() -> Self
}
```

| Method | Behaviour |
|---|---|
| `invalid_value(arg, value, reason)` | kind `InvalidValue`, message `invalid value '{value}' for '{arg}': {reason}`. For `FromArg` implementations and the derive. |
| `kind()` | the `ErrorKind`. |
| `is_display()` | true for `DisplayHelp` and `DisplayVersion`, which are not failures. |
| `message()` | the message alone, without `error:` or the usage. For help/version it is the whole text. |
| `render()` | for help/version, the text as is; otherwise `error: {message}\n`, then (when known) a blank line, the usage, `\n`, then `\nFor more information, try '--help'.\n`. |
| `print()` | `render()` to standard output for help/version, to standard error for mistakes (`print!`/`eprint!`, so a closed stream panics as they do). |
| `exit()` | `print()`, then `process::exit(0)` for help/version, `process::exit(2)` otherwise. |
| `missing_subcommand()` | kind `MissingSubcommand`, message "a subcommand is required". Used by the derive when a required subcommand enum is built from matches that have none (without usage). |
| `Display` | `render()` with trailing whitespace trimmed. |

An error made by `invalid_value` or `missing_subcommand` has no usage attached; errors raised by the parser carry the usage of the command being typed.

## Not public

`parse`, `help` and the `Level`/`State` types are private modules. `Arg` and `Command` fields are `pub(crate)`; use the getters.
