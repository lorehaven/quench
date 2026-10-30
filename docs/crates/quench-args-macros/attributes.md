---
title: Derives and attributes
parent: quench-args-macros
grand_parent: Crates
nav_order: 1
---

Everything here is read from `src/lib.rs`, `src/attrs.rs` and `src/expand.rs`. An unsupported attribute key is a compile error that lists the supported ones (see [generated](generated.md)).

## The four derives

```rust
#[proc_macro_derive(Parser, attributes(command, arg))]    pub fn derive_parser(input: TokenStream) -> TokenStream
#[proc_macro_derive(Args, attributes(command, arg))]      pub fn derive_args(input: TokenStream) -> TokenStream
#[proc_macro_derive(Subcommand, attributes(command, arg))] pub fn derive_subcommand(input: TokenStream) -> TokenStream
#[proc_macro_derive(ValueEnum, attributes(value))]        pub fn derive_value_enum(input: TokenStream) -> TokenStream
```

| Derive | On | Implements |
|---|---|---|
| `Parser` | a struct with named fields (or unit struct) | `Args`, `FromMatches`, `CommandFactory`, `Parser` |
| `Args` | a struct with named fields | `Args`, `FromMatches` |
| `Subcommand` | an enum | `Subcommand`, `FromMatches` |
| `ValueEnum` | a fieldless enum | `ValueEnum` |

Generics of the type are carried over (`impl<..> .. for Name<..> where ..`) for `Parser`, `Args` and `Subcommand`; the `ValueEnum` impl ignores generics.

## `#[command(..)]` on a `Parser` struct or on a `Subcommand` variant

Keys (a bool key may be written bare or `= true|false`):

| Key | Form | Effect |
|---|---|---|
| `name` | `name = "x"` (any expression) | command name. Parser default: `env!("CARGO_PKG_NAME")` of the crate using the derive. Variant default: the variant name in kebab-case. |
| `version` | bare, or `version = expr` | turns on `-V/--version`. Bare: `env!("CARGO_PKG_VERSION")` of the using crate. **Only on the top-level `Parser`**: on a variant it is a compile error. |
| `about` | `about = expr` | one-line description. Overrides the doc comment. |
| `long_about` | `long_about = expr` or `long_about = None` | long description. `None` suppresses the text taken from the doc comment. |
| `help_template` | `help_template = expr` | see [help templates](../quench-args/internals.md#help-templates). |
| `disable_help_subcommand` | bool | no implicit `help` subcommand. |
| `alias` | `alias = expr` (repeatable) | hidden alias. |
| `aliases` | `aliases = [a, b]` or one expression | hidden aliases. |
| `visible_alias` | `visible_alias = expr` (repeatable) | listed alias. |
| `visible_aliases` | `visible_aliases = ["a", "b"]` | listed aliases. |
| `hide` | bool | left out of the parent's command list. |
| `subcommand` | bool | on a **field** (or a tuple variant): the field holds a `Subcommand` enum. |
| `flatten` | bool | on a **field**: the field's type is an `Args` struct whose arguments are added. |

Several `#[command(..)]` attributes on one item are merged; later keys overwrite single-valued earlier ones, `alias(es)` accumulate. `#[command(..)]` on an `Args` struct or a `Subcommand` enum itself is not read (only the `Parser` struct and variants use it).

Not supported (listed in the error): everything else, e.g. `author`, `rename_all`, `arg_required_else_help`.

## `#[arg(..)]` on a field

Supported keys: `long`, `short`, `value_name`, `help`, `long_help`, `default_value`, `default_value_t`, `default_missing_value`, `num_args = 0..=1`, `require_equals`, `value_enum`, `global`, `required`, `conflicts_with`, `trailing_var_arg`, `allow_hyphen_values`, `hide`.

| Key | Form | Effect |
|---|---|---|
| `long` | bare, or `long = "name"` | `--name`. Bare: the field name (without `r#`) in kebab-case (`dry_run` becomes `--dry-run`). |
| `short` | bare, or `short = 'c'` | `-c`. Bare: the first character of the field name. |
| `value_name` | `value_name = expr` | placeholder for the value (default: field name in upper case, `-` to `_`). |
| `help` | expr | one-line help; overrides the doc comment and suppresses the doc-comment long help. |
| `long_help` | expr | text for `--help`; overrides the doc comment's long text. |
| `default_value` | expr | the default as typed text (`"10"`). Makes a plain field optional. |
| `default_value_t` | expr | any `Display` value (`300`, `true`); with `value_enum` it must be an enum value, converted through `ValueEnum::to_possible_value`. Without `value_enum` it is `ToString`ed. |
| `default_missing_value` | expr | the value used when the option is given without one; required by `num_args = 0..=1`. |
| `num_args` | `num_args = 0..=1` only | the value is optional (`--name` alone means `default_missing_value`). Any other range is a compile error. |
| `require_equals` | bool | the value, if any, must be `--name=value`. |
| `value_enum` | bool | the type (after `Option`/`Vec`) is a `ValueEnum`: values are checked and listed in help. |
| `global` | bool | accepted after subcommands. |
| `required` | bool | forces required (`required = true`) or optional (`required = false`). Default: required only for a plain `T` with no default. |
| `conflicts_with` | `conflicts_with = "id"` (repeatable, string literal) | the other argument's **id**, which is the field name. |
| `trailing_var_arg` | bool | see `Arg::trailing_var_arg`. |
| `allow_hyphen_values` | bool | see `Arg::allow_hyphen_values`. |
| `hide` | bool | left out of help. |

Field shape from the type (the last path segment is inspected, so `std::option::Option<T>` works but a type alias does not):

| Type | Kind | Reading |
|---|---|---|
| `bool` | `Kind::Flag` | `m.flag(id)`; needs `long` or `short`, else compile error "a `bool` field is a switch: give it `long` or `short`" |
| `Option<T>` | `Single`, not required | `get_one`; parsed with `FromArg` or `value_enum_arg` |
| `Vec<T>` | `Many`, not required | `get_many`, each element parsed |
| `T` | `Single`, required unless a default | `get_required(id)?` then parsed |

The argument **id** is the field name without `r#`. Field names must be named fields.

A `bool` flag with `default_value = "true"` is on when not typed (`#[arg(long, default_value = "true")] deny_warnings: bool`, from the tests). Typing the flag cannot turn it off: there is no negation. `default_value_t = true` on a `bool` is not covered by a test; it produces the default text `"true"` through `ToString` and so behaves the same.

Fields with `#[command(subcommand)]` or `#[command(flatten)]` do not read `#[arg]`:

* `subcommand` on `Option<E>`: optional; on `E`: also calls `.subcommand_required(true)`.
* `flatten`: `<T as Args>::augment_args(cmd)` and `T::from_matches(m)`.

## `#[value(..)]` on a `ValueEnum` variant

| Key | Form | Effect |
|---|---|---|
| `name` | `name = "x"` (string literal) | the spelling. Default: variant name in kebab-case. |
| `help` | `help = "text"` (string literal) | help line of the value. Default: the variant's doc comment (short text). |
| `skip` | bool | the variant is not listed and `to_possible_value` returns `None`; it cannot be typed. |

`ValueEnum::value_variants` is `&[Self::A, Self::B]` as a constant slice, so the enum must not need drop and its variants must be promotable to `'static` (plain fieldless enums are).

## Doc comments

`///` comments on the `Parser` struct, a `Subcommand` variant, a field and a `ValueEnum` variant become help.

* Lines are trimmed (one leading space and trailing whitespace dropped); consecutive non-empty lines are joined with one space into a paragraph; an empty line separates paragraphs.
* **short** text = first paragraph. A single trailing `.` is removed when the paragraph does not end in `..` and contains no `". "`. (`"Apply things."` becomes `Apply things`; `"First sentence. Second."` stays; `"Wait..."` stays.)
* **long** text = all paragraphs joined by a blank line, set only when there are two or more paragraphs.
* Command: short is `about` (unless `#[command(about)]`), long is `long_about` (unless `long_about` is given or `None`).
* Field: short is `help`; long is `long_help`. An explicit `help = ".."` suppresses the doc long text; an explicit `long_help` wins.
* `ValueEnum` variant: short text is the possible-value help (unless `#[value(help)]`).

## Names

`PascalCase` and `snake_case` become kebab-case: a lower case letter or digit followed by an upper case starts a new word; a run of capitals splits before the last when a lower case letter follows. `SetBranch` is `set-branch`, `HTTPServer` is `http-server`, `dry_run` is `dry-run`, `r#use` is `use`, `V2Api` is `v2-api`.

## Subcommand enum variants

| Variant shape | Generated |
|---|---|
| unit `List` | a subcommand with no arguments |
| named fields `Apply { .. }` | a subcommand whose arguments are the fields (same rules as a struct) |
| tuple with one field `SetBranch(BranchArgs)` | the field's `Args` are added to the subcommand |
| tuple with one field and `#[command(subcommand)]` on the variant | the subcommand holds a nested `Subcommand` enum and requires one |
| tuple with other than one field | compile error |

Building back from `Matches` matches the subcommand name; with no match, `Error::missing_subcommand()`.
