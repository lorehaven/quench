//! Reading a command line against a [`Command`].

use crate::command::{Arg, Command, Kind, PossibleValue};
use crate::error::{Error, ErrorKind};
use crate::help::{render_help, usage, version_text};
use crate::matches::Matches;

/// One command on the path from the root to the subcommand being read.
struct Level<'a> {
    cmd: &'a Command,
    /// Names from the root, e.g. `["riveter", "env", "set"]`.
    path: Vec<String>,
    matches: Matches,
    /// Positional values taken so far (a subcommand name is only recognised before any).
    positional_seen: usize,
    cursor: usize,
    /// After a `trailing_var_arg` positional started: everything else is a value.
    trailing: bool,
}

struct State<'a> {
    levels: Vec<Level<'a>>,
    /// Which arguments were typed while which command was last on the path: a global option may be given once
    /// at each level (the deepest wins), but not twice at the same one.
    typed: std::collections::HashSet<(String, usize)>,
}

impl<'a> State<'a> {
    fn top(&self) -> &Level<'a> {
        &self.levels[self.levels.len() - 1]
    }

    /// The global arguments of every command above the top one.
    fn inherited(&self) -> Vec<&'a Arg> {
        let top = self.levels.len() - 1;
        self.levels[..top]
            .iter()
            .flat_map(|level| level.cmd.args.iter())
            .filter(|arg| arg.global)
            .collect()
    }

    fn fail(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        let top = self.top();
        Error::new(kind, message).with_usage(usage(top.cmd, &top.path, &self.inherited()))
    }

    fn help(&self, long: bool) -> Error {
        let top = self.top();
        Error::new(
            ErrorKind::DisplayHelp,
            render_help(top.cmd, &top.path, &self.inherited(), long),
        )
    }

    /// The level and argument a `--name` refers to: any argument of the top command, or a global one of an
    /// ancestor.
    fn find_long(&self, name: &str) -> Option<(usize, &'a Arg)> {
        let top = self.levels.len() - 1;
        if let Some(arg) = self.levels[top].cmd.find_long(name) {
            return Some((top, arg));
        }
        (0..top).rev().find_map(|i| {
            self.levels[i]
                .cmd
                .find_long(name)
                .filter(|a| a.global)
                .map(|a| (i, a))
        })
    }

    fn find_short(&self, short: char) -> Option<(usize, &'a Arg)> {
        let top = self.levels.len() - 1;
        if let Some(arg) = self.levels[top].cmd.find_short(short) {
            return Some((top, arg));
        }
        (0..top).rev().find_map(|i| {
            self.levels[i]
                .cmd
                .find_short(short)
                .filter(|a| a.global)
                .map(|a| (i, a))
        })
    }

    fn record(&mut self, level: usize, arg: &Arg, value: Option<String>) -> Result<(), Error> {
        let typed_at = (arg.id.clone(), self.levels.len() - 1);
        if arg.kind != Kind::Many && !self.typed.insert(typed_at) {
            return Err(self.fail(
                ErrorKind::ArgumentConflict,
                format!(
                    "the argument '{}' cannot be used multiple times",
                    arg.display_name()
                ),
            ));
        }
        let Some(value) = value else {
            let m = &mut self.levels[level].matches;
            m.flags.insert(arg.id.clone());
            m.given.insert(arg.id.clone());
            return Ok(());
        };
        if !arg.possible.is_empty() && !arg.possible.iter().any(|p| p.get_name() == value) {
            let names: Vec<&str> = arg
                .possible
                .iter()
                .filter(|p| !p.is_hide_set())
                .map(PossibleValue::get_name)
                .collect();
            return Err(self.fail(
                ErrorKind::InvalidValue,
                format!(
                    "invalid value '{value}' for '{}'\n  [possible values: {}]",
                    arg.display_name(),
                    names.join(", ")
                ),
            ));
        }
        let m = &mut self.levels[level].matches;
        let values = m.values.entry(arg.id.clone()).or_default();
        if arg.kind == Kind::Many {
            values.push(value);
        } else {
            *values = vec![value];
        }
        m.given.insert(arg.id.clone());
        Ok(())
    }
}

/// Whether `token` is `-` followed by something (a flag cluster), as opposed to `-` itself or a plain word.
fn looks_like_flag(token: &str) -> bool {
    token.len() > 1 && token.starts_with('-')
}

pub fn parse(root: &Command, argv: &[String]) -> Result<Matches, Error> {
    let mut state = State {
        typed: std::collections::HashSet::new(),
        levels: vec![Level {
            cmd: root,
            path: vec![root.name.clone()],
            matches: Matches::default(),
            positional_seen: 0,
            cursor: 0,
            trailing: false,
        }],
    };
    let mut after_separator = false;
    let mut i = 0;
    while i < argv.len() {
        let token = &argv[i];
        i += 1;
        let free = !after_separator && !state.top().trailing;
        if free && token == "--" {
            after_separator = true;
            continue;
        }
        let as_value = free && looks_like_flag(token) && is_hyphen_value(&state, token);
        if as_value {
            // Falls through to `read_word`.
        } else if free
            && let Some(long) = token.strip_prefix("--")
            && read_long(&mut state, long, argv, &mut i)?
        {
            continue;
        } else if free
            && !token.starts_with("--")
            && looks_like_flag(token)
            && read_short_cluster(&mut state, token, argv, &mut i)?
        {
            continue;
        }
        read_word(&mut state, token, argv, i, free)?;
    }
    finish(state)
}

/// `--name`, `--name=value`, `--name value`. `Ok(false)` means "not a known flag, treat it as a word".
fn read_long(
    state: &mut State<'_>,
    text: &str,
    argv: &[String],
    i: &mut usize,
) -> Result<bool, Error> {
    let (name, inline) = match text.split_once('=') {
        Some((name, value)) => (name, Some(value.to_string())),
        None => (text, None),
    };
    if name == "help" {
        return Err(state.help(true));
    }
    if name == "version" && state.top().cmd.version.is_some() {
        let top = state.top();
        return Err(Error::new(
            ErrorKind::DisplayVersion,
            version_text(top.cmd, &top.path),
        ));
    }
    let Some((level, arg)) = state.find_long(name) else {
        if next_positional_takes_hyphen(state) {
            return Ok(false);
        }
        return Err(state.fail(
            ErrorKind::UnknownArgument,
            format!("unexpected argument '--{name}' found"),
        ));
    };
    if arg.kind == Kind::Flag {
        if inline.is_some() {
            return Err(state.fail(
                ErrorKind::UnexpectedValue,
                format!("unexpected value for '--{name}': it takes no value"),
            ));
        }
        state.record(level, arg, None)?;
        return Ok(true);
    }
    let value = match inline {
        Some(value) => value,
        None => optional_or_next(state, arg, argv, i)?,
    };
    state.record(level, arg, Some(value))?;
    Ok(true)
}

/// The value of an option that was not written `--name=value`: for an option with an optional value, the
/// next word if it is not a flag (and `=` is not required), else the value it takes when none is given.
fn optional_or_next(
    state: &State<'_>,
    arg: &Arg,
    argv: &[String],
    i: &mut usize,
) -> Result<String, Error> {
    if let Some(missing) = &arg.missing_value {
        if !arg.require_equals
            && let Some(next) = argv.get(*i)
            && !looks_like_flag(next)
        {
            *i += 1;
            return Ok(next.clone());
        }
        return Ok(missing.clone());
    }
    take_value(state, arg, argv, i)
}

/// The next word as the value of `arg`, which must not look like another flag.
fn take_value(
    state: &State<'_>,
    arg: &Arg,
    argv: &[String],
    i: &mut usize,
) -> Result<String, Error> {
    match argv.get(*i) {
        Some(next) if arg.allow_hyphen_values || !looks_like_flag(next) => {
            *i += 1;
            Ok(next.clone())
        }
        _ => Err(state.fail(
            ErrorKind::MissingValue,
            format!(
                "a value is required for '{}' but none was supplied",
                arg.display_name()
            ),
        )),
    }
}

/// `-v`, `-abc`, `-n5`, `-n=5`, `-n 5`.
fn read_short_cluster(
    state: &mut State<'_>,
    token: &str,
    argv: &[String],
    i: &mut usize,
) -> Result<bool, Error> {
    let chars: Vec<char> = token.chars().skip(1).collect();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        at += 1;
        if c == 'h' {
            return Err(state.help(false));
        }
        if c == 'V' && state.top().cmd.version.is_some() {
            let top = state.top();
            return Err(Error::new(
                ErrorKind::DisplayVersion,
                version_text(top.cmd, &top.path),
            ));
        }
        let Some((level, arg)) = state.find_short(c) else {
            if at == 1 && next_positional_takes_hyphen(state) {
                return Ok(false);
            }
            return Err(state.fail(
                ErrorKind::UnknownArgument,
                format!("unexpected argument '-{c}' found"),
            ));
        };
        if arg.kind == Kind::Flag {
            state.record(level, arg, None)?;
            continue;
        }
        let rest: String = chars[at..].iter().collect();
        let value = if rest.is_empty() {
            optional_or_next(state, arg, argv, i)?
        } else {
            rest.strip_prefix('=').unwrap_or(&rest).to_string()
        };
        state.record(level, arg, Some(value))?;
        return Ok(true);
    }
    Ok(true)
}

/// Whether `token` (which starts with `-`) is a value for a positional that accepts them: it is, unless it is
/// help or spells a flag the command (or a global of an ancestor) has. An attached value on an inherited short
/// option (`-eprod`) stays a value, as in clap.
fn is_hyphen_value(state: &State<'_>, token: &str) -> bool {
    if !next_positional_takes_hyphen(state) || token == "-h" || token == "--help" {
        return false;
    }
    if let Some(long) = token.strip_prefix("--") {
        let name = long.split('=').next().unwrap_or(long);
        return state.find_long(name).is_none();
    }
    let Some(first) = token.chars().nth(1) else {
        return true;
    };
    if first == 'h' {
        return false;
    }
    let local = state.top().cmd.find_short(first).is_some();
    let inherited = state.find_short(first).is_some();
    !(local || (inherited && token.chars().count() == 2))
}

fn next_positional_takes_hyphen(state: &State<'_>) -> bool {
    let top = state.top();
    top.cmd
        .positionals()
        .nth(top.cursor)
        .is_some_and(|arg| arg.allow_hyphen_values)
}

/// A subcommand name, `help ..`, or a positional value.
fn read_word(
    state: &mut State<'_>,
    word: &str,
    argv: &[String],
    i: usize,
    free: bool,
) -> Result<(), Error> {
    let top = state.levels.len() - 1;
    let cmd = state.levels[top].cmd;
    if free && state.levels[top].positional_seen == 0 {
        if let Some(sub) = cmd.find_subcommand(word) {
            let mut path = state.levels[top].path.clone();
            path.push(sub.name.clone());
            state.levels.push(Level {
                cmd: sub,
                path,
                matches: Matches::default(),
                positional_seen: 0,
                cursor: 0,
                trailing: false,
            });
            return Ok(());
        }
        if word == "help" && cmd.has_help_subcommand() {
            return Err(help_for(state, &argv[i..]));
        }
    }
    let level = &mut state.levels[top];
    let Some(arg) = cmd.positionals().nth(level.cursor) else {
        let message = if cmd.positionals().next().is_none() && !cmd.subcommands.is_empty() {
            return Err(state.fail(
                ErrorKind::UnknownSubcommand,
                format!("unrecognized subcommand '{word}'"),
            ));
        } else {
            format!("unexpected argument '{word}' found")
        };
        return Err(state.fail(ErrorKind::UnknownArgument, message));
    };
    level.positional_seen += 1;
    if arg.kind != Kind::Many {
        level.cursor += 1;
    }
    if arg.trailing {
        level.trailing = true;
    }
    state.record(top, arg, Some(word.to_string()))
}

/// `help`, `help sub`, `help sub subsub`: the help of the command the words name.
fn help_for(state: &State<'_>, words: &[String]) -> Error {
    let top = state.top();
    let mut cmd = top.cmd;
    let mut path = top.path.clone();
    let mut globals = state.inherited();
    for word in words {
        let Some(sub) = cmd.find_subcommand(word) else {
            return state.fail(
                ErrorKind::UnknownSubcommand,
                format!("unrecognized subcommand '{word}'"),
            );
        };
        globals.extend(cmd.args.iter().filter(|a| a.global));
        path.push(sub.name.clone());
        cmd = sub;
    }
    Error::new(
        ErrorKind::DisplayHelp,
        render_help(cmd, &path, &globals, true),
    )
}

/// Defaults, required arguments, conflicts, and folding the levels into one [`Matches`].
fn finish(mut state: State<'_>) -> Result<Matches, Error> {
    for index in 0..state.levels.len() {
        let cmd = state.levels[index].cmd;
        for arg in &cmd.args {
            let level = &mut state.levels[index];
            if let Some(default) = &arg.default {
                if arg.kind == Kind::Flag {
                    // A switch that is on unless... it is always on; the default says so.
                    if matches!(default.as_str(), "true" | "1" | "yes" | "on") {
                        level.matches.flags.insert(arg.id.clone());
                    }
                } else if !level.matches.values.contains_key(&arg.id) {
                    level
                        .matches
                        .values
                        .insert(arg.id.clone(), vec![default.clone()]);
                }
            }
        }
    }

    let last = state.levels.len() - 1;
    // Reported against the deepest command, which is where the user was typing.
    for index in 0..state.levels.len() {
        let cmd = state.levels[index].cmd;
        let missing: Vec<String> = cmd
            .args
            .iter()
            .filter(|arg| {
                arg.required
                    && !state.levels[index].matches.given.contains(&arg.id)
                    && !state.levels[index].matches.values.contains_key(&arg.id)
            })
            .map(Arg::display_name)
            .collect();
        if !missing.is_empty() {
            let usage_text = usage(cmd, &state.levels[index].path, &[]);
            let listing = missing
                .iter()
                .map(|m| format!("  {m}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(Error::new(
                ErrorKind::MissingRequired,
                format!("the following required arguments were not provided:\n{listing}"),
            )
            .with_usage(usage_text));
        }
        for arg in &cmd.args {
            if !state.levels[index].matches.given.contains(&arg.id) {
                continue;
            }
            for other in &arg.conflicts {
                // Only arguments typed after the same command in the line exclude each other.
                let together = (0..state.levels.len()).any(|at| {
                    state.typed.contains(&(arg.id.clone(), at))
                        && state.typed.contains(&(other.clone(), at))
                });
                if together && let Some(other_arg) = cmd.args.iter().find(|a| &a.id == other) {
                    return Err(Error::new(
                        ErrorKind::ArgumentConflict,
                        format!(
                            "the argument '{}' cannot be used with '{}'",
                            arg.display_name(),
                            other_arg.display_name()
                        ),
                    )
                    .with_usage(usage(cmd, &state.levels[index].path, &[])));
                }
            }
        }
    }

    let top = &state.levels[last];
    if top.cmd.subcommand_required && !top.cmd.subcommands.is_empty() {
        return Err(Error::new(
            ErrorKind::MissingSubcommand,
            format!(
                "'{}' requires a subcommand but one was not provided",
                top.path.join(" ")
            ),
        )
        .with_usage(usage(top.cmd, &top.path, &[])));
    }

    // Fold from the deepest level up so each parent records the subcommand chosen.
    let names: Vec<String> = state
        .levels
        .iter()
        .map(|level| level.path.last().cloned().unwrap_or_default())
        .collect();
    let mut all: Vec<Matches> = state
        .levels
        .into_iter()
        .map(|level| level.matches)
        .collect();
    let mut acc = all.pop().unwrap_or_default();
    for (index, mut parent) in all.into_iter().enumerate().rev() {
        parent.sub = Some((names[index + 1].clone(), Box::new(acc)));
        acc = parent;
    }
    Ok(acc)
}
