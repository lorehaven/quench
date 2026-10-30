---
title: Internals
parent: quench-args
grand_parent: Crates
nav_order: 4
---

Behaviour that matters to users, read from `parse.rs` and `help.rs`.

## Parsing algorithm

The words are processed left to right with a stack of levels (one per command on the path, root first). Each level has its own `Matches`, a count of positionals taken, a positional cursor and a `trailing` flag.

For each word:

1. If options are still free (no `--` seen, and the top level is not in `trailing` mode):
   * `--` alone ends option parsing; every later word is a value or subcommand name as positional rules say.
   * A word that starts with `-` and is longer than one character is a *flag-like* word. If the next positional of the top command has `allow_hyphen_values` and the word is not help and does not spell a known flag, it is taken as a positional value (`is_hyphen_value`).
   * `--name[=value]` is read as a long option; otherwise, a `-xyz` word is read as a short cluster.
2. Anything else is a *word*: if no positional has been taken at this level yet, it is looked up as a subcommand name or alias (a new level is pushed); then, if it is `help` and the command has the implicit help subcommand, the following words name the subcommand whose long help is returned (`DisplayHelp`); otherwise it fills the next positional (`unexpected argument 'w' found` when there is none, or `unrecognized subcommand` when the command has subcommands and no positionals).

So a command with both positionals and subcommands takes a word as a subcommand if it names one before any positional; after the first positional, words are always positionals.

`-` alone is a value (not flag-like). `--help`/`-h` always win, `--version`/`-V` only when a version is set. In a short cluster `-abc` each char is looked up in turn: switches are recorded; the first option that takes a value consumes the rest of the cluster (`-n5`, `-n=5`, a leading `=` is stripped) or, with nothing left, the next word. Any `h` in a cluster prints help.

### Lookup of `--name` and `-c`

First the arguments of the command at the top of the stack, then only the **global** arguments of each ancestor, nearest first. A global option typed after a subcommand therefore applies to the ancestor that declares it; its value is stored in that ancestor's `Matches`.

An unknown flag is an error (`UnknownArgument`) unless the next positional has `allow_hyphen_values`, in which case the word becomes a value.

### Repetition

For every argument that is not `Kind::Many`, typing it twice at the same level is `ArgumentConflict` ("cannot be used multiple times"). Typing a global option at several levels is allowed. `Kind::Many` options append in order. For `Kind::Single`, a value replaces any earlier one.

### Values

* An option takes the next word as its value unless that word is flag-like (`MissingValue`), or `allow_hyphen_values` is set.
* With `optional_value(missing)`: `--n=v` gives `v`; `--n` followed by a word that is not flag-like (and `require_equals` off) gives that word; otherwise `missing`. For short options `-p` behaves the same, `-px` gives `x`.
* With possible values, the value must equal one name exactly (hidden values included in the check).
* A positional of kind `Many` keeps taking words; a `Single` one advances the cursor. A positional with `trailing_var_arg` switches the level to *trailing* mode after its first value: from then on `--`, flags and subcommand names are plain values.

### After the last word (`finish`)

1. Defaults are applied per level: for non-flags, a missing value gets `[default]`; for flags, a default of `true`, `1`, `yes` or `on` turns the flag on.
2. Required arguments are checked, level by level from the root: missing ones are listed together. An argument counts as present if it was typed or has a value (a default satisfies `required`).
3. <a name="conflicts"></a>Conflicts: for each typed argument and each id in its `conflicts_with`, the error is raised when both were typed **at the same command level in the line** (same stack depth). Conflict ids are looked up among the declaring command's own arguments.
4. If the deepest command has `subcommand_required` and has subcommands, `MissingSubcommand`.
5. The levels are folded into one `Matches` tree.

Required-argument errors name the first level (from the root) that has any.

## Help layout

`render_help(cmd, path, globals, long)`:

```text
<about>                      (long: long_about, else about; omitted when none)

Usage: <path> [required options] [OPTIONS] [POSITIONALS] [COMMAND]

Arguments:                   (visible positionals)
  <NAME>   help [default: x] [possible values: a, b]

Commands:                    (visible subcommands, plus `help` when implicit)
  name, alias  about

Options:                     (own visible options, then ancestors' globals, then -h/--help and -V/--version)
  -s, --long <VALUE>  help
      --only-long     help
  -h, --help          Print help
```

* Usage always has `[OPTIONS]` (the help flag is always there). Required options are listed first by their display name. A positional shows as `<NAME>` when required, `[NAME]` otherwise, with `...` for `Many`. `[COMMAND]` becomes `<COMMAND>` when `subcommand_required`.
* The label column is padded to the widest label; rows with no help print no trailing spaces. Long-only options are indented four spaces so that they align with `-s, --long`. Text is not wrapped to the terminal width.
* The help flag's text is `Print help`; if any argument of the command or the globals has a `long_help` (or `long_about` is set, in long mode) it is `Print help (see more with '--help')` in short help and `Print help (see a summary with '-h')` in long help.
* In long help, when any argument has `long_help`, each row is printed as its label, then the long text (or help) indented by ten spaces, then a blank line.
* Annotations: `[default: ..]` for non-flags with a default; `[possible values: ..]` for visible possible values.
* The `-V` row appears only when the command has a version. Hidden arguments and commands are omitted.
* The `help` row reads `Print this message or the help of the given subcommand(s)`.

Version text: `<name> <version>\n` where `name` is the root name even for a subcommand's `--version`; subcommands made by the derive cannot have a version (`version` on a variant is a compile error), so `--version` after a subcommand is reported as an unknown argument.

### Help templates {#help-templates}

`Command::help_template` / `#[command(help_template = "..")]` replaces the whole layout. The text is filled by plain replacement of these placeholders and a final newline is added if missing:

| Placeholder | Replaced by |
|---|---|
| `{name}` | the root command name |
| `{version}` | the version or empty |
| `{about}` | the about text (long about in long mode) or empty |
| `{about-with-newline}` | the about text and a newline, or empty |
| `{usage-heading}` | `Usage:` |
| `{usage}` | the usage line without `Usage: ` |
| `{all-args}` | Arguments and Options tables with titles, no trailing newlines |
| `{options}` / `{positionals}` / `{subcommands}` | that table without its title |

Unknown `{...}` text is left as is.

## Differences from clap

From the code, `lib.rs` and the macros:

* Not implemented (compile error in the derive): `ArgAction`, `num_args` other than `0..=1`, `value_parser`, `env`, `arg_required_else_help`, `rename_all`, `author`. No colours, no wrapping.
* Options after `--` or in trailing mode are not interpreted.
* `ValueEnum` matching is case-sensitive in the derive.
* Statement from the previous page of this book (not re-verified here): when the next positional accepts hyphen values and a *global* option of an ancestor is given a value starting with `-`, clap takes the value for the option and this crate takes the option as a flag. A global option and a same-named positional of a subcommand keep separate values here.
