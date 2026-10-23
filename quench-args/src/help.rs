//! Help, usage and version text.

use std::fmt::Write as _;

use crate::command::{Arg, Command, Kind, PossibleValue};

/// `Usage: prog sub [OPTIONS] <ARG> [COMMAND]`.
pub fn usage(cmd: &Command, path: &[String], globals: &[&Arg]) -> String {
    format!("Usage: {}", usage_line(cmd, path, globals))
}

fn usage_line(cmd: &Command, path: &[String], globals: &[&Arg]) -> String {
    let mut parts = vec![path.join(" ")];
    let options: Vec<&Arg> = cmd
        .args
        .iter()
        .chain(globals.iter().copied())
        .filter(|arg| !arg.is_positional())
        .collect();
    for arg in options.iter().filter(|a| a.required) {
        parts.push(arg.display_name());
    }
    // `-h` is always there, so there is always something to put in [OPTIONS].
    parts.push("[OPTIONS]".to_string());
    for arg in cmd.positionals().filter(|a| !a.hidden) {
        let name = arg.placeholder();
        let dots = if arg.kind == Kind::Many { "..." } else { "" };
        parts.push(if arg.required {
            format!("<{name}>{dots}")
        } else {
            format!("[{name}]{dots}")
        });
    }
    if !cmd.subcommands.is_empty() {
        parts.push(if cmd.subcommand_required {
            "<COMMAND>".to_string()
        } else {
            "[COMMAND]".to_string()
        });
    }
    parts.join(" ")
}

pub fn version_text(cmd: &Command, path: &[String]) -> String {
    let name = path.first().map_or(cmd.name.as_str(), String::as_str);
    format!("{name} {}\n", cmd.version.as_deref().unwrap_or(""))
}

struct Row {
    label: String,
    help: String,
    long_help: Option<String>,
}

fn annotate(arg: &Arg) -> String {
    let mut text = arg.help.clone().unwrap_or_default();
    if let Some(default) = &arg.default
        && arg.kind != Kind::Flag
    {
        if !text.is_empty() {
            text.push(' ');
        }
        let _ = write!(text, "[default: {default}]");
    }
    let visible: Vec<&str> = arg
        .possible
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(PossibleValue::get_name)
        .collect();
    if !visible.is_empty() {
        if !text.is_empty() {
            text.push(' ');
        }
        let _ = write!(text, "[possible values: {}]", visible.join(", "));
    }
    text
}

fn option_label(arg: &Arg) -> String {
    let value = if arg.kind == Kind::Flag {
        String::new()
    } else if arg.missing_value.is_some() {
        if arg.require_equals {
            format!("[=<{}>]", arg.placeholder())
        } else {
            format!(" [<{}>]", arg.placeholder())
        }
    } else {
        format!(" <{}>", arg.placeholder())
    };
    match (&arg.long, arg.short) {
        (Some(long), Some(short)) => format!("-{short}, --{long}{value}"),
        (Some(long), None) => format!("    --{long}{value}"),
        (None, Some(short)) => format!("-{short}{value}"),
        (None, None) => arg.display_name(),
    }
}

fn positional_label(arg: &Arg) -> String {
    let name = arg.placeholder();
    match (arg.kind, arg.required) {
        (Kind::Many, true) => format!("<{name}>..."),
        (Kind::Many, false) => format!("[{name}]..."),
        (_, true) => format!("<{name}>"),
        (_, false) => format!("[{name}]"),
    }
}

fn write_rows(out: &mut String, title: &str, rows: &[Row], next_line: bool) {
    if rows.is_empty() {
        return;
    }
    let _ = writeln!(out, "{title}:");
    let width = rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0);
    for row in rows {
        if next_line {
            let _ = writeln!(out, "  {}", row.label);
            let text = row.long_help.as_deref().unwrap_or(&row.help);
            for line in text.lines() {
                if line.is_empty() {
                    out.push('\n');
                } else {
                    let _ = writeln!(out, "          {line}");
                }
            }
            out.push('\n');
        } else if row.help.is_empty() {
            let _ = writeln!(out, "  {}", row.label);
        } else {
            let _ = writeln!(out, "  {:<width$}  {}", row.label, row.help);
        }
    }
    if !next_line {
        out.push('\n');
    }
}

/// The rows of the Arguments, Options and Commands tables.
fn tables(cmd: &Command, globals: &[&Arg], long: bool) -> (Vec<Row>, Vec<Row>, Vec<Row>) {
    let any_long_help = cmd
        .args
        .iter()
        .chain(globals.iter().copied())
        .any(|a| a.long_help.is_some())
        || cmd.long_about.is_some() && long;

    let positionals: Vec<Row> = cmd
        .positionals()
        .filter(|a| !a.hidden)
        .map(|a| Row {
            label: positional_label(a),
            help: annotate(a),
            long_help: a.long_help.clone(),
        })
        .collect();

    let mut options: Vec<Row> = cmd
        .args
        .iter()
        .chain(globals.iter().copied())
        .filter(|a| !a.is_positional() && !a.hidden)
        .map(|a| Row {
            label: option_label(a),
            help: annotate(a),
            long_help: a.long_help.clone(),
        })
        .collect();
    let help_text = match (any_long_help, long) {
        (false, _) => "Print help",
        (true, false) => "Print help (see more with '--help')",
        (true, true) => "Print help (see a summary with '-h')",
    };
    options.push(Row {
        label: "-h, --help".to_string(),
        help: help_text.to_string(),
        long_help: None,
    });
    if cmd.version.is_some() {
        options.push(Row {
            label: "-V, --version".to_string(),
            help: "Print version".to_string(),
            long_help: None,
        });
    }

    let mut commands: Vec<Row> = cmd
        .subcommands
        .iter()
        .filter(|s| !s.hidden)
        .map(|s| {
            let mut label = s.name.clone();
            for alias in &s.visible_aliases {
                label.push_str(", ");
                label.push_str(alias);
            }
            Row {
                label,
                help: s.about.clone().unwrap_or_default(),
                long_help: None,
            }
        })
        .collect();
    if cmd.has_help_subcommand() {
        commands.push(Row {
            label: "help".to_string(),
            help: "Print this message or the help of the given subcommand(s)".to_string(),
            long_help: None,
        });
    }
    (positionals, options, commands)
}

/// The text `-h` (or `--help` when `long`) prints for `cmd`, reached through `path`; `globals` are the
/// ancestors' global arguments, which every subcommand accepts too.
pub fn render_help(cmd: &Command, path: &[String], globals: &[&Arg], long: bool) -> String {
    let next_line = long
        && cmd
            .args
            .iter()
            .chain(globals.iter().copied())
            .any(|a| a.long_help.is_some());
    let (positionals, options, commands) = tables(cmd, globals, long);

    let about = if long {
        cmd.long_about.as_deref().or(cmd.about.as_deref())
    } else {
        cmd.about.as_deref()
    };

    let usage_text = usage(cmd, path, globals);
    let mut positionals_text = String::new();
    write_rows(&mut positionals_text, "Arguments", &positionals, next_line);
    let mut options_text = String::new();
    write_rows(&mut options_text, "Options", &options, next_line);
    let mut commands_text = String::new();
    write_rows(&mut commands_text, "Commands", &commands, false);

    if let Some(template) = &cmd.help_template {
        let all_args = format!("{positionals_text}{options_text}");
        return fill_template(
            template,
            &[
                ("name", path.first().cloned().unwrap_or_default()),
                ("version", cmd.version.clone().unwrap_or_default()),
                ("about", about.unwrap_or_default().to_string()),
                (
                    "about-with-newline",
                    about.map_or_else(String::new, |a| format!("{a}\n")),
                ),
                ("usage-heading", "Usage:".to_string()),
                (
                    "usage",
                    usage_text.trim_start_matches("Usage: ").to_string(),
                ),
                ("all-args", all_args.trim_end().to_string()),
                ("options", strip_title(&options_text, "Options:")),
                ("positionals", strip_title(&positionals_text, "Arguments:")),
                ("subcommands", strip_title(&commands_text, "Commands:")),
            ],
        );
    }

    let mut out = String::new();
    if let Some(about) = about {
        let _ = writeln!(out, "{about}\n");
    }
    let _ = writeln!(out, "{usage_text}\n");
    out.push_str(&positionals_text);
    out.push_str(&commands_text);
    out.push_str(&options_text);
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

fn strip_title(text: &str, title: &str) -> String {
    text.strip_prefix(title)
        .unwrap_or(text)
        .trim_matches('\n')
        .to_string()
}

fn fill_template(template: &str, values: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}
