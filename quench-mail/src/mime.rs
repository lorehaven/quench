//! The wire-format primitives: RFC 2047 encoded words, quoted-printable,
//! header folding. Pure functions over text, no I/O.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

/// Longest line a header may be folded to (RFC 5322 recommends 78).
const FOLD_AT: usize = 78;

/// Raw bytes per encoded word. `=?UTF-8?B?` + base64 + `?=` must fit in 75
/// characters, which leaves 60 base64 characters, i.e. 45 bytes.
const WORD_BYTES: usize = 45;

/// CRLF line endings, whatever the input used (`\n`, `\r\n` or a stray `\r`).
pub fn normalize_crlf(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 40);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str("\r\n");
            }
            '\n' => out.push_str("\r\n"),
            c => out.push(c),
        }
    }
    out
}

/// True for text that can appear in a header as-is: printable ASCII plus
/// space and tab.
pub fn is_plain_header_text(text: &str) -> bool {
    text.chars().all(|c| c == '\t' || (' '..='~').contains(&c))
}

/// UTF-8 base64 encoded words for `text`, split on character boundaries.
pub fn encoded_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in text.chars() {
        if chunk.len() + c.len_utf8() > WORD_BYTES {
            words.push(encode_word(&chunk));
            chunk.clear();
        }
        chunk.push(c);
    }
    if !chunk.is_empty() {
        words.push(encode_word(&chunk));
    }
    words
}

fn encode_word(chunk: &str) -> String {
    format!("=?UTF-8?B?{}?=", STANDARD.encode(chunk.as_bytes()))
}

/// An unstructured header value (`Subject`): as-is when plain, otherwise
/// encoded words separated by spaces (which folding may turn into line breaks).
pub fn unstructured(text: &str) -> String {
    if is_plain_header_text(text) {
        text.to_string()
    } else {
        encoded_words(text).join(" ")
    }
}

/// A display name (RFC 5322 `phrase`): bare when it is only atoms and spaces,
/// a quoted string when it holds specials, encoded words when not ASCII.
pub fn phrase(name: &str) -> String {
    if !is_plain_header_text(name) {
        return encoded_words(name).join(" ");
    }
    if name.chars().any(|c| "()<>[]:;@\\,.\"".contains(c)) {
        let mut quoted = String::with_capacity(name.len() + 2);
        quoted.push('"');
        for c in name.chars() {
            if c == '"' || c == '\\' {
                quoted.push('\\');
            }
            quoted.push(c);
        }
        quoted.push('"');
        return quoted;
    }
    name.to_string()
}

/// `Name: value`, folded at spaces to at most [`FOLD_AT`] columns where a
/// space allows it. A token longer than that is left whole rather than split.
pub fn fold_header(name: &str, value: &str) -> String {
    let mut out = String::with_capacity(name.len() + value.len() + 8);
    out.push_str(name);
    out.push(':');
    let mut line_len = name.len() + 1;
    for token in value.split(' ').filter(|t| !t.is_empty()) {
        if line_len + 1 + token.len() > FOLD_AT && line_len > name.len() + 1 {
            out.push_str("\r\n ");
            line_len = 1;
        } else {
            out.push(' ');
            line_len += 1;
        }
        out.push_str(token);
        line_len += token.len();
    }
    out
}

/// Whether `text` (already CRLF-normalised) can travel as `7bit`: ASCII, no
/// NUL, no line over 998 characters.
pub fn is_7bit(text: &str) -> bool {
    text.is_ascii() && !text.contains('\0') && text.split("\r\n").all(|line| line.len() <= 998)
}

/// Quoted-printable (RFC 2045) of CRLF-normalised text: lines of at most 76
/// characters, `=` escapes for anything outside printable ASCII, and trailing
/// whitespace on a line escaped so mail relays cannot strip it.
pub fn quoted_printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 8);
    let mut lines = text.split("\r\n").peekable();
    while let Some(line) = lines.next() {
        let bytes = line.as_bytes();
        let last = bytes.len().saturating_sub(1);
        let tokens: Vec<String> = bytes
            .iter()
            .enumerate()
            .map(|(i, &b)| match b {
                b' ' | b'\t' if i == last => format!("={b:02X}"),
                b' ' | b'\t' | 33..=60 | 62..=126 => (b as char).to_string(),
                _ => format!("={b:02X}"),
            })
            .collect();

        let mut column = 0;
        for (i, token) in tokens.iter().enumerate() {
            // A soft break costs one column for its `=`, unless this token
            // ends the line, when 76 columns are available.
            let limit = if i + 1 == tokens.len() { 76 } else { 75 };
            if column + token.len() > limit {
                out.push_str("=\r\n");
                column = 0;
            }
            out.push_str(token);
            column += token.len();
        }
        if lines.peek().is_some() {
            out.push_str("\r\n");
        }
    }
    out
}
