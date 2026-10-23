//! Splitting and quoting command lines the way a POSIX shell reads words (`shlex`'s role).

use std::borrow::Cow;

/// Splits `input` into words the way a POSIX shell reads them.
///
/// Whitespace separates; `'..'` is literal; `".."` honours `\\`, `\"`, `\$`, `` \` `` and a line continuation;
/// a backslash outside quotes escapes the next character; `#` at the start of a word begins a comment that
/// runs to the end of the line.
///
/// `None` when a quote or an escape is left open. `''` is one empty word.
#[must_use]
pub fn split(input: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '#' {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        let mut word = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() {
                break;
            }
            chars.next();
            match c {
                '\'' => loop {
                    match chars.next()? {
                        '\'' => break,
                        other => word.push(other),
                    }
                },
                '"' => loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            '\n' => {}
                            escaped @ ('\\' | '"' | '$' | '`') => word.push(escaped),
                            other => {
                                word.push('\\');
                                word.push(other);
                            }
                        },
                        other => word.push(other),
                    }
                },
                '\\' => match chars.next()? {
                    '\n' => {}
                    escaped => word.push(escaped),
                },
                other => word.push(other),
            }
        }
        words.push(word);
    }
    Some(words)
}

/// `word` as one shell word: unchanged when it is made only of safe characters, otherwise single-quoted.
#[must_use]
pub fn quote(word: &str) -> Cow<'_, str> {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:@%+,".contains(c))
    {
        return Cow::Borrowed(word);
    }
    let mut out = String::with_capacity(word.len() + 2);
    out.push('\'');
    for c in word.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    Cow::Owned(out)
}

/// The words as one command line that [`split`] reads back unchanged.
#[must_use]
pub fn join<I, S>(words: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    words
        .into_iter()
        .map(|word| quote(word.as_ref()).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(input: &str) -> Vec<String> {
        split(input).expect("balanced")
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(
            words("  cargo   build\t--release \n"),
            ["cargo", "build", "--release"]
        );
        assert!(words("").is_empty());
        assert!(words("   ").is_empty());
    }

    #[test]
    fn quotes_group_words() {
        assert_eq!(
            words(r#"echo "a b" 'c d' e"f g"h"#),
            ["echo", "a b", "c d", "ef gh"]
        );
        assert_eq!(words("''"), [""]);
        assert_eq!(words(r#"a "" b"#), ["a", "", "b"]);
        assert_eq!(split("it's"), None);
    }

    #[test]
    fn escapes() {
        assert_eq!(words(r"a\ b c"), ["a b", "c"]);
        assert_eq!(
            words(r#""a\"b" "c\\d" "e\$f" "g\xh""#),
            [r#"a"b"#, r"c\d", "e$f", r"g\xh"]
        );
        assert_eq!(words("'a\\b'"), [r"a\b"]);
        assert_eq!(words("a\\\nb"), ["ab"]);
        assert_eq!(words("\"a\\\nb\""), ["ab"]);
    }

    #[test]
    fn comments_run_to_the_end_of_the_line() {
        assert_eq!(words("a # comment\nb"), ["a", "b"]);
        assert_eq!(words("a#b"), ["a#b"]);
        assert_eq!(words("# only"), Vec::<String>::new());
    }

    #[test]
    fn unbalanced_input_is_rejected() {
        for bad in ["'open", "\"open", "trailing\\", "\"esc\\", "a 'b"] {
            assert_eq!(split(bad), None, "{bad}");
        }
    }

    #[test]
    fn quote_leaves_safe_words_alone() {
        assert_eq!(
            quote("path/to-file_1.txt"),
            Cow::Borrowed("path/to-file_1.txt")
        );
        assert_eq!(quote(""), "''");
        assert_eq!(quote("a b"), "'a b'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("$HOME"), "'$HOME'");
    }

    #[test]
    fn join_round_trips_through_split() {
        let original = [
            "echo",
            "a b",
            "",
            "it's",
            "$x",
            "multi\nline",
            "back\\slash",
            "#c",
            "é",
        ];
        assert_eq!(words(&join(original)), original);
    }
}
