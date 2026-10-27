//! HTML escaping, and the checks that keep attacker-controlled strings from becoming markup.

/// Tags written without a closing tag.
pub const VOID_TAGS: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

#[must_use]
pub fn is_void(tag: &str) -> bool {
    VOID_TAGS.contains(&tag)
}

/// Escapes text for use between tags.
#[must_use]
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes a value for use inside double quotes (single quotes are escaped too, so either works).
#[must_use]
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            '\0' => out.push('\u{fffd}'),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes the text of a `<script>` or `<style>` element so it cannot end the element early.
#[must_use]
pub fn escape_raw_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' && chars.peek() == Some(&'/') {
            out.push_str("<\\");
        } else if c == '<' && chars.peek() == Some(&'!') {
            out.push_str("\\x3c");
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `tag` is safe to write as an element name.
#[must_use]
pub fn is_valid_tag(tag: &str) -> bool {
    let mut chars = tag.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Whether `name` may be set as an attribute: a plain name that is not an inline event handler.
#[must_use]
pub fn is_valid_attr_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.'))
        && !name.to_ascii_lowercase().starts_with("on")
}

/// Attributes whose value is a URL.
const URL_ATTRS: [&str; 8] = [
    "href",
    "src",
    "action",
    "formaction",
    "poster",
    "data",
    "xlink:href",
    "srcset",
];

/// What a blocked URL becomes.
pub const BLOCKED_URL: &str = "about:invalid#blocked";

/// Returns the value to set for `name`: unchanged, or [`BLOCKED_URL`] if it is a URL attribute
/// holding a `javascript:`, `vbscript:` or non-image `data:` URL.
#[must_use]
pub fn sanitize_attr_value<'a>(name: &str, value: &'a str) -> &'a str {
    if !URL_ATTRS.contains(&name.to_ascii_lowercase().as_str()) {
        return value;
    }
    // Browsers ignore ASCII whitespace and control characters inside the scheme.
    let compact: String = value
        .chars()
        .filter(|c| !c.is_ascii_control() && !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let blocked = compact.starts_with("javascript:")
        || compact.starts_with("vbscript:")
        || (compact.starts_with("data:") && !compact.starts_with("data:image/"));
    if blocked { BLOCKED_URL } else { value }
}
