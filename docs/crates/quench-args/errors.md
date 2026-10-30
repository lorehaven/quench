---
title: Errors
parent: quench-args
grand_parent: Crates
nav_order: 3
---

`ErrorKind` and the message of each failure, from `parse.rs`, `error.rs` and `lib.rs`. Mistakes are rendered as `error: <message>`, a blank line, `Usage: ...`, a blank line and `For more information, try '--help'.` (see `Error::render`). Process exit status through `Error::exit`: 0 for the two display kinds, 2 for all others.

| Kind | When | Message |
|---|---|---|
| `DisplayHelp` | `-h`, `--help`, or `help [subcommand..]` | the help text (`-h`: short, `--help` and `help ..`: long). Not a failure. |
| `DisplayVersion` | `-V` / `--version`, only when the command has a version | `<root name> <version>\n` |
| `UnknownArgument` | a flag no command on the path accepts, or a stray positional | `unexpected argument '--name' found`, `unexpected argument '-c' found`, `unexpected argument 'word' found` |
| `UnknownSubcommand` | a word where only subcommands fit (the command has subcommands and no positionals), or `help nope` | `unrecognized subcommand 'word'` |
| `MissingRequired` | a required argument neither typed nor defaulted | `the following required arguments were not provided:` followed by one `  <display name>` line each (display name: `--env <ENV>`, `-v`, `<TARGET>`, `<TARGET>...`); or from `Matches::get_required`: `the argument '<id>' was not provided` |
| `MissingSubcommand` | `subcommand_required` and none given | `'<path>' requires a subcommand but one was not provided`; from the derive: `a subcommand is required` |
| `MissingValue` | an option that takes a value is last, or the next word looks like a flag (and `allow_hyphen_values` is off) | `a value is required for '--name <NAME>' but none was supplied` |
| `InvalidValue` | not in the possible values, or `FromArg`/`FromStr` failed | `invalid value 'x' for '--scope <SCOPE>'\n  [possible values: a, b]` (possible values) or `invalid value 'x' for '--n <N>': <FromStr error>` |
| `ArgumentConflict` | a non-`Many` argument typed twice at one level; or two `conflicts_with` arguments typed at the same level | `the argument '--name <NAME>' cannot be used multiple times` / `the argument '--a' cannot be used with '--b'` |
| `UnexpectedValue` | `--flag=value` for a switch | `unexpected value for '--flag': it takes no value` |
| `InvalidUtf8` | a word that is not UTF-8 | `invalid UTF-8 was detected in one or more arguments: <lossy text>` |

Notes

* For a short switch given a value (`-v=1`), the cluster reader treats `=1` as the next characters, so the result is `UnknownArgument` (`unexpected argument '-=' found`), not `UnexpectedValue` (only `--flag=value` yields `UnexpectedValue`). Inferred from `read_short_cluster`; not covered by a test.
* A parse error never panics. Panics only come from `Command::debug_assert` and from `Error::print` writing to a closed stream.
* `missing_subcommand()` and `invalid_value` errors created by the derive have no usage text, so the printed form is `error: ...` followed by the `--help` pointer.
