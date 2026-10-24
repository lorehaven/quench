//! Step patterns: cucumber expressions (`I have {int} cucumber(s)`, `{string}`, `a/b`) and plain regexes.
//!
//! An expression is compiled to a regular expression; [`Matcher::matches`] returns the captured arguments in
//! order. `{string}` yields the text between its quotes (with `\"` and `\'` unescaped).

use quench_regex::Regex;

/// How an argument is read from the regex captures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capture {
    /// One group.
    Plain,
    /// `{string}`: two groups (double or single quoted), at most one set.
    Quoted,
}

/// A compiled step pattern.
#[derive(Debug, Clone)]
pub struct Matcher {
    regex: Regex,
    captures: Vec<Capture>,
    /// For a regex pattern: every capture group is an argument.
    raw: bool,
}

const STRING: &str = r#""((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)'"#;

impl Matcher {
    /// Compiles `pattern`, as a regex if `regex` is set and as a cucumber expression otherwise.
    ///
    /// # Errors
    ///
    /// A message when the expression is malformed (unknown `{type}`, unbalanced braces) or the regex does not compile.
    pub fn new(pattern: &str, regex: bool) -> Result<Self, String> {
        if regex {
            let compiled =
                Regex::new(pattern).map_err(|e| format!("bad regex `{pattern}`: {e}"))?;
            return Ok(Self {
                regex: compiled,
                captures: Vec::new(),
                raw: true,
            });
        }
        let mut captures = Vec::new();
        let source = translate(pattern, &mut captures)?;
        let compiled = Regex::new(&format!("^{source}$"))
            .map_err(|e| format!("bad expression `{pattern}`: {e}"))?;
        Ok(Self {
            regex: compiled,
            captures,
            raw: false,
        })
    }

    /// The arguments if `text` matches the whole pattern.
    #[must_use]
    pub fn matches(&self, text: &str) -> Option<Vec<String>> {
        let found = self.regex.captures(text)?;
        if self.raw {
            return Some(
                (1..self.regex.captures_len())
                    .map(|i| {
                        found
                            .get(i)
                            .map_or_else(String::new, |m| m.as_str().to_string())
                    })
                    .collect(),
            );
        }
        let mut group = 1;
        let mut out = Vec::new();
        for capture in &self.captures {
            match capture {
                Capture::Plain => {
                    out.push(
                        found
                            .get(group)
                            .map_or_else(String::new, |m| m.as_str().to_string()),
                    );
                    group += 1;
                }
                Capture::Quoted => {
                    let raw = found
                        .get(group)
                        .or_else(|| found.get(group + 1))
                        .map_or("", |m| m.as_str());
                    out.push(unescape(raw));
                    group += 2;
                }
            }
        }
        Some(out)
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next @ ('"' | '\'' | '\\')) => out.push(next),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parameter(name: &str, captures: &mut Vec<Capture>) -> Result<String, String> {
    Ok(match name {
        "string" => {
            captures.push(Capture::Quoted);
            format!("(?:{STRING})")
        }
        "int" => {
            captures.push(Capture::Plain);
            r"([-+]?\d+)".to_string()
        }
        "float" => {
            captures.push(Capture::Plain);
            r"([-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?)".to_string()
        }
        "word" => {
            captures.push(Capture::Plain);
            r"([^\s]+)".to_string()
        }
        "" => {
            captures.push(Capture::Plain);
            "(.*)".to_string()
        }
        other => return Err(format!("unknown parameter type `{{{other}}}`")),
    })
}

/// One alternative of a word: literal text, `(optional)` text and `{parameters}`.
fn alternative(text: &str, captures: &mut Vec<Capture>) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next() {
                    out.push_str(&quench_regex::escape(&next.to_string()));
                }
            }
            '(' => {
                let mut inner = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == ')' {
                        closed = true;
                        break;
                    }
                    inner.push(next);
                }
                if !closed {
                    return Err("an optional `(` is never closed".to_string());
                }
                out.push_str("(?:");
                out.push_str(&quench_regex::escape(&inner));
                out.push_str(")?");
            }
            '{' => {
                let mut name = String::new();
                let mut closed = false;
                for next in chars.by_ref() {
                    if next == '}' {
                        closed = true;
                        break;
                    }
                    name.push(next);
                }
                if !closed {
                    return Err("a parameter `{` is never closed".to_string());
                }
                out.push_str(&parameter(&name, captures)?);
            }
            c => out.push_str(&quench_regex::escape(&c.to_string())),
        }
    }
    Ok(out)
}

/// Splits `word` at unescaped `/` outside parentheses and braces.
fn alternatives(word: &str) -> Vec<String> {
    let mut parts = vec![String::new()];
    let mut depth = 0usize;
    let mut escaped = false;
    for c in word.chars() {
        let last = parts.last_mut().expect("never empty");
        if escaped {
            last.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => {
                last.push(c);
                escaped = true;
            }
            '(' | '{' => {
                depth += 1;
                last.push(c);
            }
            ')' | '}' => {
                depth = depth.saturating_sub(1);
                last.push(c);
            }
            '/' if depth == 0 => parts.push(String::new()),
            c => last.push(c),
        }
    }
    parts
}

fn translate(pattern: &str, captures: &mut Vec<Capture>) -> Result<String, String> {
    let mut out = String::new();
    let mut word = String::new();
    let mut depth = 0usize;
    let mut escaped = false;
    let mut flush = |word: &mut String, out: &mut String| -> Result<(), String> {
        if word.is_empty() {
            return Ok(());
        }
        let parts = alternatives(word);
        word.clear();
        if parts.len() == 1 {
            out.push_str(&alternative(&parts[0], captures)?);
        } else {
            let mut compiled = Vec::new();
            for part in &parts {
                compiled.push(alternative(part, captures)?);
            }
            out.push_str("(?:");
            out.push_str(&compiled.join("|"));
            out.push(')');
        }
        Ok(())
    };
    for c in pattern.chars() {
        if escaped {
            word.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' => {
                word.push(c);
                escaped = true;
            }
            '(' | '{' => {
                depth += 1;
                word.push(c);
            }
            ')' | '}' => {
                depth = depth.saturating_sub(1);
                word.push(c);
            }
            ' ' if depth == 0 => {
                flush(&mut word, &mut out)?;
                out.push(' ');
            }
            c => word.push(c),
        }
    }
    flush(&mut word, &mut out)?;
    Ok(out)
}
