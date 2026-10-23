# Quench Args

`quench-args` (crate `quench_args`) replaces `clap` for the Forge tools: command-line parsing with `#[derive(Parser, Args, Subcommand, ValueEnum)]` and clap's attribute names, so a tool switches by changing its `use`. Its only dependency is `quench-args-macros`, which uses `syn`/`quote`/`proc-macro2` at compile time only.

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
    #[command(visible_alias = "r")]
    Render {
        #[arg(long, value_enum, default_value_t = Scope::All)]
        scope: Scope,
        targets: Vec<String>,
    },
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum Scope { All, Mutable }

let cli = Cli::parse();
```

## What is supported

- **Derives.** `Parser` (a whole command line: `parse`, `try_parse`, `parse_from`, `try_parse_from`, `command()`), `Args` (a reusable group, flattened with `#[command(flatten)]` or as a tuple variant `Login(LoginArgs)`), `Subcommand` (struct, unit and tuple variants; nested with `#[command(subcommand)]`, optional or required by the field's type), `ValueEnum` (kebab-case names, `#[value(name = "..", help = "..", skip)]`).
- **`#[command(..)]`**: `name`, `version` (bare or a string), `about`, `long_about` (`None` suppresses the doc text), `help_template`, `disable_help_subcommand`, `alias`, `aliases`, `visible_alias`, `visible_aliases`, `hide`, `subcommand`, `flatten`.
- **`#[arg(..)]`**: `long` / `long = ".."`, `short` / `short = 'c'`, `value_name`, `help`, `long_help`, `default_value`, `default_value_t` (any `Display` value, or an enum with `value_enum`), `value_enum`, `global`, `required`, `conflicts_with`, `trailing_var_arg`, `allow_hyphen_values`, `hide`. A field's type decides its shape: `bool` is a switch, `Option<T>` an optional value, `Vec<T>` any number (a repeatable option, or the rest of the positionals), anything else is required unless it has a default. A field with neither `long` nor `short` is positional. Any `FromStr` type works (`String`, `PathBuf`, integers, floats, your own).
- **Anything else is a compile error** that lists what is supported, never a silent no-op.
- **Doc comments** become help: the first paragraph (lines joined, a single trailing period dropped) is the short text `-h` shows; with more paragraphs the whole text is what `--help` shows.
- **Parsing.** `--long`, `--long=value`, `--long value`, `-s`, `-svalue`, `-s=value`, `-abc` clusters, `--` ending options, `-` as a value, subcommand aliases, global options before or after the subcommand (a repeat at another level overrides), `help [subcommand..]`, `-h`/`--help`, `-V`/`--version`. A repeated single-value option or switch is an error (clap's rule); a positional that is `trailing_var_arg` takes everything after its first value, flags included; `allow_hyphen_values` lets unknown `-x` words be values. A command with both positionals and subcommands (`foreman conveyor` versus `foreman start`) takes a word as a subcommand if it names one before any positional.
- **Introspection.** `Cli::command()` gives the `Command` tree: `get_name`, `get_about`, `get_version`, `get_subcommands`, `get_visible_aliases`, `get_all_aliases`, `find_subcommand`, `get_arguments`, `render_help`, `render_long_help`, and `debug_assert()` (duplicate names and ids, clashes with `-h`/`-V`, conflicts with unknown arguments, a switch with no flag to spell it). The same builder API (`Command::new`, `Arg::new`, `Matches`) is public for tools that do not use the derives.

## Help and errors

Help is laid out like clap's: description, `Usage:`, then `Arguments:`, `Commands:` and `Options:` tables, with `[default: ..]` and `[possible values: ..]` appended; long-only options are indented under the short ones, `--help` puts each argument's long text on its own lines, and subcommand help lists the ancestors' global options. `help_template` takes `{name}`, `{version}`, `{about}`, `{about-with-newline}`, `{usage-heading}`, `{usage}`, `{all-args}`, `{options}`, `{positionals}`, `{subcommands}`. Text is not wrapped to the terminal width.

Errors read `error: <message>`, the usage, and `For more information, try '--help'.`; `Error::kind()` tells them apart (`UnknownArgument`, `UnknownSubcommand`, `MissingRequired`, `MissingSubcommand`, `MissingValue`, `InvalidValue`, `ArgumentConflict`, `UnexpectedValue`, `InvalidUtf8`, plus `DisplayHelp` and `DisplayVersion`, which are not failures). `Error::exit()` prints (help and version to standard output) and exits with status 0 for those two and 2 otherwise, which is what `parse()` does.

## Differences from clap

Not implemented: `ArgAction` variants (counts, appends beyond `Vec`), `num_args`, `value_parser`, `env =`, `arg_required_else_help`, `rename_all`, `author`, colours in help and wrapping. When the next positional accepts hyphen values and a *global* option of an ancestor is given a value starting with `-` there, clap takes the value for the option and this crate takes the option as a flag (a corner no tool here uses). The id collision clap has between a global option and a same-named positional of a subcommand (`riveter --env` and `env set <env>`) does not exist here: each keeps its own value.

## Testing

Unit tests next to the code and a derive-expansion test suite in `quench-args-macros`; behaviour was compared with clap 4 on 1.2 million generated command lines over three command definitions (accept/reject and the parsed values identical, apart from the corner above).

[Home](../README.md)
