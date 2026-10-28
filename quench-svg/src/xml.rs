//! The small part of XML an SVG file needs: elements, attributes, text, comments and entities.

/// An element and what is in it.
#[derive(Debug, Clone, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Self>,
}

impl Element {
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// How deep elements may nest (a hostile file must not exhaust the stack).
const MAX_DEPTH: usize = 64;

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let number = entity.strip_prefix('#').and_then(|n| {
                    n.strip_prefix(['x', 'X'])
                        .map_or_else(|| n.parse().ok(), |h| u32::from_str_radix(h, 16).ok())
                });
                match number.and_then(char::from_u32) {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[..=end]),
                }
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Parses `text` and returns its root element.
pub fn parse(text: &str) -> Result<Element, &'static str> {
    let mut at = 0;
    let mut stack: Vec<Element> = Vec::new();
    let bytes = text.as_bytes();
    let mut root: Option<Element> = None;
    while at < bytes.len() {
        let Some(lt) = text[at..].find('<') else {
            break;
        };
        at += lt;
        let rest = &text[at..];
        if rest.starts_with("<!--") {
            at += rest.find("-->").ok_or("a comment is not closed")? + 3;
        } else if rest.starts_with("<![CDATA[") {
            at += rest.find("]]>").ok_or("CDATA is not closed")? + 3;
        } else if rest.starts_with("<?") {
            at += rest.find("?>").ok_or("a declaration is not closed")? + 2;
        } else if rest.starts_with("<!") {
            // A doctype (with an internal subset in brackets, perhaps): entities are not read from it.
            let mut depth = 0;
            let mut end = None;
            for (i, c) in rest.char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    '>' if depth <= 0 => {
                        end = Some(i);
                        break;
                    }
                    _ => {}
                }
            }
            at += end.ok_or("a doctype is not closed")? + 1;
        } else if let Some(closing) = rest.strip_prefix("</") {
            let end = closing.find('>').ok_or("a tag is not closed")?;
            let name = closing[..end].trim();
            let done = stack.pop().ok_or("a closing tag without an opening one")?;
            if done.name != name.rsplit(':').next().unwrap_or(name) {
                return Err("tags are not nested");
            }
            at += end + 3;
            match stack.last_mut() {
                Some(parent) => parent.children.push(done),
                None => root = Some(done),
            }
        } else {
            let (element, length, empty) = open_tag(rest)?;
            at += length;
            if empty {
                match stack.last_mut() {
                    Some(parent) => parent.children.push(element),
                    None => root = Some(element),
                }
            } else {
                if stack.len() >= MAX_DEPTH {
                    return Err("elements are nested too deep");
                }
                stack.push(element);
            }
        }
        if root.is_some() && stack.is_empty() {
            break;
        }
    }
    if !stack.is_empty() {
        return Err("an element is not closed");
    }
    root.ok_or("there is no element")
}

/// Reads `<name a="b" ...>` (or `.../>`): the element, the bytes used and whether it is empty.
fn open_tag(rest: &str) -> Result<(Element, usize, bool), &'static str> {
    let mut chars = rest.char_indices().skip(1).peekable();
    let mut name = String::new();
    while let Some(&(_, c)) = chars.peek() {
        if c.is_whitespace() || c == '>' || c == '/' {
            break;
        }
        name.push(c);
        chars.next();
    }
    if name.is_empty() {
        return Err("a tag has no name");
    }
    let mut element = Element {
        name: name.rsplit(':').next().unwrap_or(&name).to_string(),
        ..Element::default()
    };
    loop {
        while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
            chars.next();
        }
        let Some(&(i, c)) = chars.peek() else {
            return Err("a tag is not closed");
        };
        match c {
            '>' => return Ok((element, i + 1, false)),
            '/' => {
                chars.next();
                return match chars.next() {
                    Some((j, '>')) => Ok((element, j + 1, true)),
                    _ => Err("a tag is not closed"),
                };
            }
            _ => {
                let mut key = String::new();
                while let Some(&(_, c)) = chars.peek() {
                    if c == '=' || c.is_whitespace() || c == '>' || c == '/' {
                        break;
                    }
                    key.push(c);
                    chars.next();
                }
                while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                    chars.next();
                }
                if chars.next().map(|(_, c)| c) != Some('=') {
                    return Err("an attribute has no value");
                }
                while chars.peek().is_some_and(|(_, c)| c.is_whitespace()) {
                    chars.next();
                }
                let quote = match chars.next() {
                    Some((_, q @ ('"' | '\''))) => q,
                    _ => return Err("an attribute value is not quoted"),
                };
                let mut value = String::new();
                loop {
                    match chars.next() {
                        Some((_, c)) if c == quote => break,
                        Some((_, c)) => value.push(c),
                        None => return Err("an attribute value is not closed"),
                    }
                }
                // `xlink:href` and `href` are the same to us.
                let key = key.rsplit(':').next().unwrap_or(&key).to_string();
                element.attrs.push((key, unescape(&value)));
            }
        }
    }
}
