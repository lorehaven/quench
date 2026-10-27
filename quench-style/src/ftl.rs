//! The part of Fluent (`.ftl`) that translation bundles need: message ids and
//! their text, with `{ $variable }` kept as a `{$variable}` token.
//!
//! Terms (`-brand = ...`), comments, attributes (`.title = ...`) and every
//! other kind of placeable (functions, selectors, string literals, references)
//! are recognised, so that a file containing them parses, but contribute
//! nothing: a message whose value uses them gets an empty string in their
//! place, which is what the client-side lookup expects.

use std::fmt;

/// A message with a value (messages with only attributes are not listed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub value: String,
}

/// The text is not valid Fluent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtlError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for FtlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for FtlError {}

fn fail<T>(line: usize, message: &str) -> Result<T, FtlError> {
    Err(FtlError {
        line: line + 1,
        message: message.to_string(),
    })
}

const fn is_id_start(c: char) -> bool {
    c.is_ascii_alphabetic()
}

const fn is_id_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Parses a whole file.
pub fn parse(source: &str) -> Result<Vec<Message>, FtlError> {
    let normalized = source.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut messages = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            i += 1;
        } else if let Some(rest) = line.strip_prefix('#') {
            let rest = rest.trim_start_matches('#');
            if line.len() - rest.len() > 3 || !(rest.is_empty() || rest.starts_with(' ')) {
                return fail(i, "a comment is `#`, `##` or `###` followed by a space");
            }
            i += 1;
        } else if line.starts_with(|c: char| is_id_start(c) || c == '-') {
            let (entry, next) = entry(&lines, i)?;
            if let Some(message) = entry {
                messages.push(message);
            }
            i = next;
        } else {
            return fail(i, "expected a message, a term or a comment");
        }
    }
    Ok(messages)
}

/// One message or term starting at `start`; returns it (messages only) and the
/// index of the first line after it.
fn entry(lines: &[&str], start: usize) -> Result<(Option<Message>, usize), FtlError> {
    let head = lines[start];
    let is_term = head.starts_with('-');
    let body = if is_term { &head[1..] } else { head };
    let id_len = body.find(|c: char| !is_id_char(c)).unwrap_or(body.len());
    let id = &body[..id_len];
    if id.is_empty() || !id.starts_with(is_id_start) {
        return fail(start, "expected an identifier");
    }
    let after = body[id_len..].trim_start_matches(' ');
    let Some(first) = after.strip_prefix('=') else {
        return fail(start, "expected `=` after the identifier");
    };

    // The entry runs over every following line that is indented, or blank
    // lines that an indented line follows - and, while a `{` is still open,
    // over anything (a selector's closing `}` may sit in the first column).
    let mut end = start + 1;
    let mut last_content = start;
    let mut open = Depth::default();
    open.scan(first);
    while end < lines.len() {
        let line = lines[end];
        // Fluent lets a placeable or an attribute start in the first column too.
        let continues = line.starts_with(' ') || line.starts_with('{') || is_attribute_start(line);
        if open.inside() || (continues && !line.trim().is_empty()) {
            open.scan(line);
            last_content = end;
            end += 1;
        } else if line.trim().is_empty() {
            end += 1;
        } else {
            break;
        }
    }
    let block = &lines[start + 1..=last_content];

    // Split off attributes: an indented line starting with `.name =` outside braces.
    let mut depth = Depth::default();
    depth.scan(first);
    let mut value_end = block.len();
    for (n, line) in block.iter().enumerate() {
        if depth.inside() {
            depth.scan(line);
            continue;
        }
        let trimmed = line.trim_start_matches(' ');
        if is_attribute_start(trimmed) {
            value_end = n;
            break;
        }
        depth.scan(line);
    }
    if depth.unclosed() && value_end == block.len() {
        return fail(start, "unclosed `{`");
    }
    let (value_lines, attribute_lines) = block.split_at(value_end);

    if !attribute_lines.is_empty() {
        let mut attr_depth = Depth::default();
        for line in attribute_lines {
            attr_depth.scan(line);
        }
        if attr_depth.unclosed() {
            return fail(start, "unclosed `{` in an attribute");
        }
    }

    let value = pattern(first, value_lines, start)?;
    if value.is_none() && attribute_lines.is_empty() {
        return fail(start, "a message needs a value or attributes");
    }
    let message = (!is_term)
        .then(|| {
            value.map(|value| Message {
                id: id.to_string(),
                value,
            })
        })
        .flatten();
    Ok((message, last_content + 1))
}

fn is_attribute_start(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix('.') else {
        return false;
    };
    let id_len = rest.find(|c: char| !is_id_char(c)).unwrap_or(rest.len());
    id_len > 0
        && rest.starts_with(is_id_start)
        && rest[id_len..].trim_start_matches(' ').starts_with('=')
}

/// Brace depth across lines, ignoring braces inside string literals.
#[derive(Default)]
struct Depth {
    level: usize,
    in_string: bool,
    /// The last character that was not a blank, outside strings.
    previous: Option<char>,
}

/// A `"` starts a string literal only where an expression can begin; inside a
/// selector's variant text it is just a quote.
const fn opens_string(previous: Option<char>) -> bool {
    matches!(previous, Some('{' | '(' | ',' | ':' | '>'))
}

impl Depth {
    fn scan(&mut self, text: &str) {
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if self.in_string {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => self.in_string = false,
                    _ => {}
                }
            } else {
                match c {
                    '{' => self.level += 1,
                    '}' => self.level = self.level.saturating_sub(1),
                    '"' if self.level > 0 && opens_string(self.previous) => self.in_string = true,
                    _ => {}
                }
                if !c.is_whitespace() {
                    self.previous = Some(c);
                }
            }
        }
    }

    const fn inside(&self) -> bool {
        self.level > 0
    }

    const fn unclosed(&self) -> bool {
        self.level > 0 || self.in_string
    }
}

/// The text of a value: `first` is what followed `=` on the head line, `rest`
/// the indented lines after it. `None` when there is no text at all.
fn pattern(first: &str, rest: &[&str], head_line: usize) -> Result<Option<String>, FtlError> {
    // Which lines begin outside a placeable (only those are text lines)?
    let mut depth = Depth::default();
    depth.scan(first);
    let mut text_line = Vec::with_capacity(rest.len());
    for line in rest {
        text_line.push(!depth.inside());
        depth.scan(line);
    }

    let indent_of = |line: &str| line.len() - line.trim_start_matches(' ').len();
    let common = rest
        .iter()
        .zip(&text_line)
        .filter(|(line, text)| **text && !line.trim().is_empty())
        .map(|(line, _)| indent_of(line))
        .min()
        .unwrap_or(0);

    // Re-assemble the value as one string with the common indentation removed,
    // placeable lines left whole.
    let mut assembled = String::from(first.trim_start_matches(' '));
    for (line, text) in rest.iter().zip(&text_line) {
        assembled.push('\n');
        if *text {
            assembled.push_str(line.get(common.min(indent_of(line))..).unwrap_or(""));
        } else {
            assembled.push_str(line);
        }
    }

    let mut out = String::new();
    let mut chars = assembled.chars();
    let mut seen_text = false;
    let mut pending_newlines = 0usize;
    // Text before a trailing placeable keeps its trailing blanks.
    let mut protected = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                flush_newlines(&mut out, &mut pending_newlines, seen_text);
                let mut level = 1;
                let mut in_string = false;
                let mut previous = Some('{');
                let mut inner = String::new();
                for d in chars.by_ref() {
                    if in_string {
                        inner.push(d);
                        if d == '"' {
                            in_string = false;
                        }
                        continue;
                    }
                    let before = previous;
                    if !d.is_whitespace() {
                        previous = Some(d);
                    }
                    match d {
                        '"' if opens_string(before) => {
                            in_string = true;
                            inner.push(d);
                        }
                        '{' => {
                            level += 1;
                            inner.push(d);
                        }
                        '}' => {
                            level -= 1;
                            if level == 0 {
                                break;
                            }
                            inner.push(d);
                        }
                        _ => inner.push(d),
                    }
                }
                if level != 0 {
                    return fail(head_line, "unclosed `{`");
                }
                seen_text = true;
                if let Some(name) = variable(&inner) {
                    out.push_str("{$");
                    out.push_str(name);
                    out.push('}');
                }
                protected = out.len();
            }
            '}' => {
                return fail(
                    head_line,
                    "unbalanced `}`; write a literal brace as `{ \"}\" }`",
                );
            }
            '\n' => pending_newlines += 1,
            other => {
                flush_newlines(&mut out, &mut pending_newlines, seen_text);
                seen_text = true;
                out.push(other);
            }
        }
    }
    // trailing whitespace is not part of the value
    if !seen_text {
        return Ok(None);
    }
    let tail = out.split_off(protected.min(out.len()));
    out.push_str(tail.trim_end_matches([' ', '\n']));
    Ok(Some(out))
}

/// Line breaks before the first text are dropped; later ones are kept.
fn flush_newlines(out: &mut String, pending: &mut usize, seen_text: bool) {
    if seen_text {
        for _ in 0..*pending {
            out.push('\n');
        }
    }
    *pending = 0;
}

/// `$name` (with surrounding blanks) -> `name`.
fn variable(expression: &str) -> Option<&str> {
    let name = expression.trim().strip_prefix('$')?;
    (!name.is_empty() && name.starts_with(is_id_start) && name.chars().all(is_id_char))
        .then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(source: &str) -> Vec<(String, String)> {
        parse(source)
            .unwrap_or_else(|e| panic!("{source:?}: {e}"))
            .into_iter()
            .map(|m| (m.id, m.value))
            .collect()
    }

    fn value(source: &str) -> String {
        values(source).remove(0).1
    }

    fn rejects(source: &str) -> String {
        parse(source).expect_err(source).to_string()
    }

    #[quench_test::test]
    fn simple_messages() {
        assert_eq!(
            values("hello = Hello, world!\nbye = Bye\n"),
            [
                ("hello".into(), "Hello, world!".into()),
                ("bye".into(), "Bye".into())
            ]
        );
        assert_eq!(value("a-b_c9 = ok"), "ok");
        assert_eq!(value("k =    padded   \n"), "padded");
        assert_eq!(
            value("unicode = Zażółć gęślą jaźń 中文 😀\n"),
            "Zażółć gęślą jaźń 中文 😀"
        );
        assert_eq!(parse("").unwrap(), []);
        assert_eq!(parse("\n\n   \n").unwrap(), []);
    }

    #[quench_test::test]
    fn comments_and_terms_are_skipped() {
        let source = "# a\n## group\n### resource\n#\nkey = v\n-brand = Quench\n-multi =\n    Line\n    two\nlast = l\n";
        assert_eq!(
            values(source),
            [("key".into(), "v".into()), ("last".into(), "l".into())]
        );
        assert!(rejects("#nospace\nk = v").contains("comment"));
        assert!(rejects("#### four\nk = v").contains("comment"));
    }

    #[quench_test::test]
    fn variables_are_kept_as_tokens() {
        assert_eq!(value("g = Hello, { $name }!"), "Hello, {$name}!");
        assert_eq!(value("g = {$a} and { $b } x{$c}"), "{$a} and {$b} x{$c}");
        assert_eq!(value("g = { $with-dash_1 }"), "{$with-dash_1}");
    }

    #[quench_test::test]
    fn other_placeables_contribute_nothing() {
        assert_eq!(value("a = { -brand } is here"), " is here");
        assert_eq!(value("a = { other-msg } tail"), " tail");
        assert_eq!(
            value("a = { NUMBER($n, minimumFractionDigits: 2) } done"),
            " done"
        );
        assert_eq!(value("a = lit { \"literal { brace\" } end"), "lit  end");
        assert_eq!(value("a = { $ } x"), " x");
        assert_eq!(value("a = { $9bad } x"), " x");
        assert_eq!(value("a = { \"{\" }brace{ \"}\" }"), "brace");
        assert_eq!(value("a = { $x } { { \"nested\" } }"), "{$x} ");
    }

    #[quench_test::test]
    fn multiline_values_are_dedented() {
        assert_eq!(
            value("m =\n    Line one\n    Line two\n"),
            "Line one\nLine two"
        );
        assert_eq!(value("m = first\n    second\n"), "first\nsecond");
        assert_eq!(value("m =\n    a\n\n    after blank\n"), "a\n\nafter blank");
        assert_eq!(
            value("m =\n    base\n      deeper\n    base\n"),
            "base\n  deeper\nbase"
        );
        assert_eq!(
            value("m =\n\n\n    leading blanks dropped\n\n\n"),
            "leading blanks dropped"
        );
        assert_eq!(value("m = a\n  b\n    c\n"), "a\nb\n  c");
        assert_eq!(value("m =\n    { $x }\n    tail\n"), "{$x}\ntail");
    }

    #[quench_test::test]
    fn selectors_span_lines() {
        let source = "s = { $n ->\n    [one] One item\n   *[other] { $n } items\n}\nafter = x\n";
        assert_eq!(
            values(source),
            [("s".into(), String::new()), ("after".into(), "x".into())]
        );
        let tail = "s = { $a ->\n   *[x] A\n    [y] B\n }\n    tail text\n";
        assert_eq!(value(tail), "\ntail text");
        assert_eq!(
            value("s = { $n ->\n    [one] say \"hi\"\n   *[other] x\n}\n"),
            ""
        );
    }

    #[quench_test::test]
    fn a_placeable_may_start_in_the_first_column() {
        assert_eq!(value("m = a\n{ $b }\n"), "a\n{$b}");
        assert_eq!(value("m =\n{ $b ->\n *[x] y\n}\n"), "");
    }

    #[quench_test::test]
    fn attributes() {
        assert_eq!(
            values("a = Value\n    .title = T\n    .alt = Alt { $x }\nb = B\n"),
            [("a".into(), "Value".into()), ("b".into(), "B".into())]
        );
        // a message with only attributes has no value of its own
        assert_eq!(
            values("a =\n    .title = Only attr\nb = B\n"),
            [("b".into(), "B".into())]
        );
        assert_eq!(
            values("a = V\n.title = at column zero\n"),
            [("a".into(), "V".into())]
        );
        assert_eq!(value("a = V\n    .t = multi\n      line attr\n"), "V");
        assert!(rejects("a = V\n    .t = { $x\n").contains("unclosed"));
    }

    #[quench_test::test]
    fn line_endings() {
        assert_eq!(
            values("a = one\r\nb =\r\n    two\r\n"),
            [("a".into(), "one".into()), ("b".into(), "two".into())]
        );
    }

    #[quench_test::test]
    fn errors_name_the_line() {
        assert_eq!(
            rejects("ok = 1\nbad\n"),
            "line 2: expected `=` after the identifier"
        );
        assert_eq!(
            rejects("= nothing"),
            "line 1: expected a message, a term or a comment"
        );
        assert!(rejects("  indented = x").contains("expected a message"));
        assert!(rejects("k = { $x").contains("unclosed"));
        assert!(rejects("k = }").contains("unbalanced"));
        assert!(rejects("k = text\n  more }").contains("unbalanced"));
        assert!(rejects("k =\n").contains("needs a value or attributes"));
        assert!(rejects("-t\n").contains("expected `=`"));
        assert!(rejects("9bad = x").contains("expected a message"));
        assert!(rejects("k = { \"unterminated\n}").contains("unclosed"));
        assert!(rejects("-9 = x").contains("expected an identifier"));
        assert!(rejects("-1x = y").contains("identifier"));
    }

    #[quench_test::test]
    fn error_type_displays() {
        let e = parse("bad\n").unwrap_err();
        assert_eq!(
            (e.line, e.message.as_str()),
            (1, "expected `=` after the identifier")
        );
        let _: &dyn std::error::Error = &e;
    }
}
