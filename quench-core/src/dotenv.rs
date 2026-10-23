//! `.env` files (`dotenvy`'s role): `KEY=value` lines that fill in environment variables which are not already set.
//!
//! Syntax: blank lines and `#` comments are skipped; an optional `export ` prefix is allowed; a value is bare
//! (ends at a ` #` comment, trailing space trimmed; `\n` is a newline, `\ ` a space), `'single quoted'` (literal) or `"double quoted"` (escapes
//! `\n \r \t \\ \" \$`). Quoted values may span lines. `$NAME` and `${NAME}` expand in bare and double-quoted
//! values from earlier lines of the file or the process environment (unset gives nothing).

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::env;

/// A line that is not `KEY=value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based.
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, ".env line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// Why loading failed.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Parse(ParseError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cannot read .env: {error}"),
            Self::Parse(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ParseError> for Error {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

/// The `(key, value)` pairs of `text`, in order, with `$VAR` references expanded.
///
/// # Errors
///
/// [`ParseError`] for a line without `=`, an empty or invalid key, or an unterminated quote.
pub fn parse(text: &str) -> Result<Vec<(String, String)>, ParseError> {
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut seen: HashMap<String, String> = HashMap::new();
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let mut line = 1;
    while at < chars.len() {
        // Start of a logical line.
        let start_line = line;
        while at < chars.len() && chars[at] != '\n' && chars[at].is_whitespace() {
            at += 1;
        }
        if at >= chars.len() {
            break;
        }
        if chars[at] == '\n' {
            at += 1;
            line += 1;
            continue;
        }
        if chars[at] == '#' {
            while at < chars.len() && chars[at] != '\n' {
                at += 1;
            }
            continue;
        }
        let mut key = String::new();
        while at < chars.len() && chars[at] != '=' && chars[at] != '\n' {
            key.push(chars[at]);
            at += 1;
        }
        let key = key.trim();
        let key = key.strip_prefix("export ").map_or(key, str::trim_start);
        if at >= chars.len() || chars[at] != '=' {
            return Err(ParseError {
                line: start_line,
                message: format!("`{key}` has no `=`"),
            });
        }
        if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.') {
            return Err(ParseError {
                line: start_line,
                message: format!("`{key}` is not a valid name"),
            });
        }
        at += 1;
        let lookup = |name: &str| {
            seen.get(name)
                .cloned()
                .or_else(|| std::env::var(name).ok())
                .unwrap_or_default()
        };
        let value = read_value(&chars, &mut at, &mut line, &lookup).map_err(|message| ParseError {
            line: start_line,
            message,
        })?;
        seen.insert(key.to_string(), value.clone());
        pairs.push((key.to_string(), value));
    }
    Ok(pairs)
}

/// Reads one value starting right after `=`, leaving `at` on the line's end.
fn read_value(
    chars: &[char],
    at: &mut usize,
    line: &mut usize,
    lookup: &dyn Fn(&str) -> String,
) -> Result<String, String> {
    while *at < chars.len() && chars[*at] != '\n' && chars[*at].is_whitespace() {
        *at += 1;
    }
    let mut value = String::new();
    // A value may be several adjacent pieces: `a"b"'c'`.
    while *at < chars.len() && chars[*at] != '\n' {
        match chars[*at] {
            '\'' => {
                *at += 1;
                loop {
                    let c = *chars.get(*at).ok_or("unterminated single quote")?;
                    *at += 1;
                    match c {
                        '\'' => break,
                        '\n' => {
                            *line += 1;
                            value.push(c);
                        }
                        _ => value.push(c),
                    }
                }
            }
            '"' => {
                *at += 1;
                loop {
                    let c = *chars.get(*at).ok_or("unterminated double quote")?;
                    *at += 1;
                    match c {
                        '"' => break,
                        '\\' => {
                            let escaped = *chars.get(*at).ok_or("unterminated double quote")?;
                            *at += 1;
                            match escaped {
                                'n' => value.push('\n'),
                                'r' => value.push('\r'),
                                't' => value.push('\t'),
                                '\\' | '"' | '$' => value.push(escaped),
                                other => {
                                    value.push('\\');
                                    value.push(other);
                                }
                            }
                        }
                        '$' => expand(chars, at, &mut value, lookup),
                        '\n' => {
                            *line += 1;
                            value.push(c);
                        }
                        _ => value.push(c),
                    }
                }
            }
            '#' if value.is_empty() || chars[*at - 1].is_whitespace() => {
                while *at < chars.len() && chars[*at] != '\n' {
                    *at += 1;
                }
            }
            '\\' if chars.get(*at + 1).is_some_and(|c| *c != '\n') => {
                let escaped = chars[*at + 1];
                value.push(if escaped == 'n' { '\n' } else { escaped });
                *at += 2;
            }
            '$' => {
                *at += 1;
                expand(chars, at, &mut value, lookup);
            }
            '\r' if matches!(chars.get(*at + 1), Some('\n') | None) => *at += 1,
            c => {
                value.push(c);
                *at += 1;
            }
        }
    }
    // Bare text keeps no trailing blanks (quoted text ending in a space is not reached here, as its
    // closing quote is last).
    let trimmed = value.trim_end_matches([' ', '\t', '\r']);
    let trailing_quote = chars[..*at].iter().rev().find(|c| !c.is_whitespace());
    let value = if matches!(trailing_quote, Some('"' | '\'')) {
        value
    } else {
        trimmed.to_string()
    };
    Ok(value)
}

/// After a `$`: `${NAME}` or `$NAME`; a lone `$` stays.
fn expand(chars: &[char], at: &mut usize, out: &mut String, lookup: &dyn Fn(&str) -> String) {
    if chars.get(*at) == Some(&'{') {
        if let Some(end) = chars[*at..].iter().position(|c| *c == '}') {
            let name: String = chars[*at + 1..*at + end].iter().collect();
            out.push_str(&lookup(&name));
            *at += end + 1;
            return;
        }
        out.push('$');
        return;
    }
    let name: String = chars[*at..]
        .iter()
        .take_while(|c| c.is_ascii_alphanumeric() || **c == '_')
        .collect();
    if name.is_empty() {
        out.push('$');
        return;
    }
    out.push_str(&lookup(&name));
    *at += name.chars().count();
}

/// Sets each pair of the file at `path` that is not already set in the environment.
///
/// # Errors
///
/// [`Error::Io`] when it cannot be read, [`Error::Parse`] when it is malformed (nothing is set then).
pub fn load_from(path: impl AsRef<Path>) -> Result<(), Error> {
    let text = std::fs::read_to_string(path)?;
    for (key, value) in parse(&text)? {
        if std::env::var_os(&key).is_none() {
            env::set(key, value);
        }
    }
    Ok(())
}

/// The nearest `.env` in the current directory or one of its parents.
#[must_use]
pub fn find() -> Option<PathBuf> {
    let start = std::env::current_dir().ok()?;
    start
        .ancestors()
        .map(|dir| dir.join(".env"))
        .find(|candidate| candidate.is_file())
}

/// [`load_from`] the nearest `.env`; `Ok(None)` when there is none.
///
/// # Errors
///
/// As [`load_from`].
pub fn load() -> Result<Option<PathBuf>, Error> {
    find().map_or(Ok(None), |path| load_from(&path).map(|()| Some(path)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmp::TempDir;

    fn pairs(text: &str) -> Vec<(String, String)> {
        parse(text).expect("valid")
    }

    fn kv(key: &str, value: &str) -> (String, String) {
        (key.to_string(), value.to_string())
    }

    #[test]
    fn plain_lines_comments_and_export() {
        let got = pairs("# top\n\nA=1\n  B = two words  \nexport C=3\nD=\nE=x # note\nF=a#b\n");
        assert_eq!(
            got,
            [kv("A", "1"), kv("B", "two words"), kv("C", "3"), kv("D", ""), kv("E", "x"), kv("F", "a#b")]
        );
    }

    #[test]
    fn quotes() {
        let got = pairs("A='lit $X \\n'\nB=\"tab\\there \\\"q\\\" \\$X \\\\\"\nC=\"multi\nline\"\nD=\"keeps trailing \"\nE=a\"b c\"'d'\n");
        assert_eq!(
            got,
            [
                kv("A", "lit $X \\n"),
                kv("B", "tab\there \"q\" $X \\"),
                kv("C", "multi\nline"),
                kv("D", "keeps trailing "),
                kv("E", "ab cd"),
            ]
        );
    }

    #[test]
    fn variables_expand_from_earlier_lines_and_the_environment() {
        // SAFETY of the test: a name unique to it.
        env::set("QUENCH_DOTENV_TEST_HOME", "/home/x");
        let got = pairs(
            "BASE=/srv\nA=$BASE/a\nB=\"${BASE}/b\"\nC=$QUENCH_DOTENV_TEST_HOME/c\nD=$NOPE_NOT_SET_ANYWHERE|\nE='$BASE'\nF=cost$\nG=${open\n",
        );
        assert_eq!(got[1], kv("A", "/srv/a"));
        assert_eq!(got[2], kv("B", "/srv/b"));
        assert_eq!(got[3], kv("C", "/home/x/c"));
        assert_eq!(got[4], kv("D", "|"));
        assert_eq!(got[5], kv("E", "$BASE"));
        assert_eq!(got[6], kv("F", "cost$"));
        assert_eq!(got[7], kv("G", "${open"));
        env::remove("QUENCH_DOTENV_TEST_HOME");
    }

    #[test]
    fn crlf_line_endings_work() {
        assert_eq!(pairs("A=1\r\nB='x'\r\n"), [kv("A", "1"), kv("B", "x")]);
    }

    #[test]
    fn malformed_lines_name_their_line() {
        assert_eq!(parse("A=1\nnoequals\n").unwrap_err().line, 2);
        assert!(parse("=v").unwrap_err().to_string().contains("not a valid name"));
        assert!(parse("A=\"open").unwrap_err().message.contains("unterminated"));
        assert!(parse("A='open").unwrap_err().message.contains("unterminated"));
        assert!(parse("A=\"x\\").unwrap_err().message.contains("unterminated"));
        assert!(parse("bad key=1").is_err());
        assert_eq!(parse("A=\"1\nB=2\n").unwrap_err().line, 1);
    }

    #[test]
    fn loading_never_overrides_what_is_set() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join(".env");
        std::fs::write(&file, "QUENCH_DOTENV_A=from-file\nQUENCH_DOTENV_B=from-file\n").unwrap();
        env::set("QUENCH_DOTENV_A", "from-env");
        env::remove("QUENCH_DOTENV_B");
        load_from(&file).unwrap();
        assert_eq!(env::get_or("QUENCH_DOTENV_A", ""), "from-env");
        assert_eq!(env::get_or("QUENCH_DOTENV_B", ""), "from-file");
        env::remove("QUENCH_DOTENV_A");
        env::remove("QUENCH_DOTENV_B");

        assert!(matches!(load_from(dir.path().join("missing")), Err(Error::Io(_))));
        std::fs::write(&file, "broken\n").unwrap();
        let error = load_from(&file).unwrap_err();
        assert!(matches!(error, Error::Parse(_)));
        assert!(error.to_string().contains("line 1"));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn find_and_load_search_upwards() {
        // Runs against the real working directory, so only checks consistency.
        match find() {
            Some(path) => assert!(path.ends_with(".env")),
            None => assert!(matches!(load(), Ok(None))),
        }
    }
}
