//! Scanners shared by the block and inline parsers: URLs, link parts and raw HTML tags.

pub const fn is_escapable(c: u8) -> bool {
    c.is_ascii_punctuation()
}

/// Backslash escapes and character references resolved.
pub fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() && is_escapable(bytes[i + 1]) => {
                out.push(bytes[i + 1] as char);
                i += 2;
            }
            b'&' => {
                if let Some((text, used)) = crate::entities::reference(&s[i..], true) {
                    out.push_str(&text);
                    i += used;
                } else {
                    out.push('&');
                    i += 1;
                }
            }
            _ => {
                let c = s[i..].chars().next().unwrap_or('\u{FFFD}');
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}

/// Escapes text content (`&`, `<`, `>`).
pub fn escape_text(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

/// Escapes an attribute value (also `"`).
pub fn escape_attr(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// Removes trailing newlines and the lines made only of spaces before them.
pub fn strip_trailing_blank_lines(s: &str) -> &str {
    let mut end = s.len();
    loop {
        let trimmed = s[..end].trim_end_matches(' ');
        if trimmed.ends_with('\n') {
            end = trimmed.len() - 1;
        } else {
            return &s[..end];
        }
    }
}

/// Percent-encodes what a URL cannot hold, keeping existing `%XX` escapes and URL punctuation.
pub fn encode_uri(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let keep = b.is_ascii_alphanumeric()
            || b";/?:@&=+$,-_.!~*'()#".contains(&b)
            || (b == b'%'
                && bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit));
        if keep {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Case-folded, whitespace-collapsed link label (without its brackets).
pub fn normalize_label(raw: &str) -> String {
    let inner = &raw[1..raw.len() - 1];
    let collapsed = inner
        .split([' ', '\t', '\r', '\n'])
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.to_lowercase().to_uppercase()
}

/// The length of a link label at the start of `s` (`[` … `]`, no unescaped brackets), or 0.
pub fn link_label(s: &str) -> usize {
    let b = s.as_bytes();
    if b.first() != Some(&b'[') {
        return 0;
    }
    let mut i = 1;
    while i < b.len() && i <= 1000 {
        match b[i] {
            b'\\' if i + 1 < b.len() => i += 2,
            b'[' => return 0,
            b']' => return i + 1,
            _ => i += 1,
        }
    }
    0
}

/// A link destination at the start of `s`: the unescaped text and its length.
pub fn link_destination(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    if b.first() == Some(&b'<') {
        let mut i = 1;
        while i < b.len() {
            match b[i] {
                b'>' => return Some((unescape(&s[1..i]), i + 1)),
                b'\n' | b'<' | 0 => return None,
                b'\\' if i + 1 < b.len() && is_escapable(b[i + 1]) => i += 2,
                _ => i += 1,
            }
        }
        return None;
    }
    let mut i = 0;
    let mut open = 0usize;
    while i < b.len() {
        match b[i] {
            b'\\' if i + 1 < b.len() && is_escapable(b[i + 1]) => i += 2,
            b'(' => {
                open += 1;
                i += 1;
            }
            b')' => {
                if open == 0 {
                    break;
                }
                open -= 1;
                i += 1;
            }
            c if c <= b' ' || c == 0x7F => break,
            _ => i += 1,
        }
    }
    if open != 0 || (i == 0 && b.first() != Some(&b')')) {
        return None;
    }
    Some((unescape(&s[..i]), i))
}

/// A link title at the start of `s`: the unescaped text and its length.
pub fn link_title(s: &str) -> Option<(String, usize)> {
    let b = s.as_bytes();
    let (close, nested) = match b.first()? {
        b'"' => (b'"', false),
        b'\'' => (b'\'', false),
        b'(' => (b')', true),
        _ => return None,
    };
    let mut i = 1;
    while i < b.len() {
        match b[i] {
            b'\\' if i + 1 < b.len() && is_escapable(b[i + 1]) => i += 2,
            c if c == close => return Some((unescape(&s[1..i]), i + 1)),
            b'(' if nested => return None,
            0 => return None,
            _ => i += 1,
        }
    }
    None
}

const fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

const fn name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// The length of an HTML open tag at the start of `s`, or 0.
pub fn open_tag(s: &str) -> usize {
    let b = s.as_bytes();
    if b.len() < 2 || b[0] != b'<' || !b[1].is_ascii_alphabetic() {
        return 0;
    }
    let mut i = 2;
    while i < b.len() && name_char(b[i]) {
        i += 1;
    }
    loop {
        let before = i;
        while i < b.len() && is_space(b[i]) {
            i += 1;
        }
        if i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == b'_' || b[i] == b':') && i > before
        {
            i += 1;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric() || matches!(b[i], b':' | b'.' | b'_' | b'-'))
            {
                i += 1;
            }
            // Optional value.
            let mut j = i;
            while j < b.len() && is_space(b[j]) {
                j += 1;
            }
            if j < b.len() && b[j] == b'=' {
                j += 1;
                while j < b.len() && is_space(b[j]) {
                    j += 1;
                }
                match b.get(j) {
                    Some(&q @ (b'"' | b'\'')) => {
                        let Some(end) = b[j + 1..].iter().position(|&c| c == q) else {
                            return 0;
                        };
                        i = j + 1 + end + 1;
                    }
                    Some(_) => {
                        let start = j;
                        while j < b.len()
                            && !is_space(b[j])
                            && !matches!(b[j], b'"' | b'\'' | b'=' | b'<' | b'>' | b'`')
                        {
                            j += 1;
                        }
                        if j == start {
                            return 0;
                        }
                        i = j;
                    }
                    None => return 0,
                }
            }
            continue;
        }
        i = before;
        break;
    }
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    if b.get(i) == Some(&b'/') {
        i += 1;
    }
    if b.get(i) == Some(&b'>') { i + 1 } else { 0 }
}

/// The length of an HTML closing tag at the start of `s`, or 0.
pub fn close_tag(s: &str) -> usize {
    let b = s.as_bytes();
    if b.len() < 3 || b[0] != b'<' || b[1] != b'/' || !b[2].is_ascii_alphabetic() {
        return 0;
    }
    let mut i = 3;
    while i < b.len() && name_char(b[i]) {
        i += 1;
    }
    while i < b.len() && is_space(b[i]) {
        i += 1;
    }
    if b.get(i) == Some(&b'>') { i + 1 } else { 0 }
}

/// The length of any inline raw HTML at the start of `s` (tag, comment, processing instruction,
/// declaration, CDATA), or 0.
pub fn inline_html(s: &str) -> usize {
    let n = open_tag(s);
    if n > 0 {
        return n;
    }
    let n = close_tag(s);
    if n > 0 {
        return n;
    }
    if let Some(rest) = s.strip_prefix("<!--") {
        if rest.starts_with('>') {
            return 5;
        }
        if rest.starts_with("->") {
            return 6;
        }
        return rest.find("-->").map_or(0, |i| 4 + i + 3);
    }
    if let Some(rest) = s.strip_prefix("<?") {
        return rest.find("?>").map_or(0, |i| 2 + i + 2);
    }
    if let Some(rest) = s.strip_prefix("<![CDATA[") {
        return rest.find("]]>").map_or(0, |i| 9 + i + 3);
    }
    if let Some(rest) = s.strip_prefix("<!") {
        let letters = rest.bytes().take_while(u8::is_ascii_uppercase).count();
        if letters > 0 && rest[letters..].starts_with(|c: char| c.is_ascii_whitespace()) {
            return rest.find('>').map_or(0, |i| 2 + i + 1);
        }
    }
    0
}

/// Whether an HTML block of the given type (1-7) starts with `s`.
pub fn html_block_start(kind: u8, s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    let tag_end = |after: &str| {
        after.is_empty() || after.starts_with(|c: char| c.is_ascii_whitespace() || c == '>')
    };
    match kind {
        1 => ["<script", "<pre", "<textarea", "<style"]
            .iter()
            .any(|t| lower.strip_prefix(t).is_some_and(tag_end)),
        2 => s.starts_with("<!--"),
        3 => s.starts_with("<?"),
        4 => s
            .strip_prefix("<!")
            .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_alphabetic())),
        5 => s.starts_with("<![CDATA["),
        6 => {
            const NAMES: [&str; 62] = [
                "address",
                "article",
                "aside",
                "base",
                "basefont",
                "blockquote",
                "body",
                "caption",
                "center",
                "col",
                "colgroup",
                "dd",
                "details",
                "dialog",
                "dir",
                "div",
                "dl",
                "dt",
                "fieldset",
                "figcaption",
                "figure",
                "footer",
                "form",
                "frame",
                "frameset",
                "h1",
                "h2",
                "h3",
                "h4",
                "h5",
                "h6",
                "head",
                "header",
                "hr",
                "html",
                "iframe",
                "legend",
                "li",
                "link",
                "main",
                "menu",
                "menuitem",
                "nav",
                "noframes",
                "ol",
                "optgroup",
                "option",
                "p",
                "param",
                "search",
                "section",
                "summary",
                "table",
                "tbody",
                "td",
                "tfoot",
                "th",
                "thead",
                "title",
                "tr",
                "track",
                "ul",
            ];
            let body = lower
                .strip_prefix("</")
                .or_else(|| lower.strip_prefix('<'))
                .unwrap_or("");
            let name_len = body.bytes().take_while(u8::is_ascii_alphanumeric).count();
            NAMES.contains(&&body[..name_len]) && {
                let after = &body[name_len..];
                after.is_empty()
                    || after.starts_with(|c: char| c.is_ascii_whitespace())
                    || after.starts_with('>')
                    || after.starts_with("/>")
            }
        }
        7 => {
            let n = open_tag(s).max(close_tag(s));
            n > 0 && s[n..].trim().is_empty() && !s[n..].contains('\n')
        }
        _ => false,
    }
}

/// Whether `s` contains the end condition of an HTML block of the given type (1-5).
pub fn html_block_end(kind: u8, s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    match kind {
        1 => ["</script>", "</pre>", "</textarea>", "</style>"]
            .iter()
            .any(|t| lower.contains(t)),
        2 => s.contains("-->"),
        3 => s.contains("?>"),
        4 => s.contains('>'),
        5 => s.contains("]]>"),
        _ => false,
    }
}

/// The length of an autolink body (`scheme:...` or an email address) in `s`, which starts after `<`: the
/// part before the closing `>`, and whether it is an email.
pub fn autolink(s: &str) -> Option<(usize, bool)> {
    let end = s.find(['>', '<', ' ', '\n', '\t'])?;
    if !s[end..].starts_with('>') {
        return None;
    }
    let body = &s[..end];
    if let Some(colon) = body.find(':') {
        let scheme = &body[..colon];
        if (2..=32).contains(&scheme.len())
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-'))
            && body.bytes().all(|c| c > b' ' && c != 0x7F)
        {
            return Some((end, false));
        }
    }
    let at = body.find('@')?;
    let (local, domain) = (&body[..at], &body[at + 1..]);
    let local_ok = !local.is_empty()
        && local
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&c));
    let domain_ok = !domain.is_empty()
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        });
    (local_ok && domain_ok).then_some((end, true))
}
