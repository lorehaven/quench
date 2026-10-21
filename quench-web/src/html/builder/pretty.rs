//! A small HTML reader + indenting writer, used by `pretty_print_html`.
//!
//! It understands what this crate's own builders emit (and hand-written
//! fragments of the same kind): start / end tags, attributes, void
//! elements, comments, doctype, raw-text (`script`, `style`) and
//! escapable-raw-text (`textarea`, `title`) bodies, and character
//! references. It is *not* the HTML5 tree-construction algorithm: there is
//! no implied end-tag handling (`<p>` is not auto-closed by a following
//! `<div>`), so input has to be well-nested. Like a browser it does wrap a
//! fragment in `<html><head></head><body>…</body></html>`.

use super::element::html_escape;
use std::fmt::Write as _;

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
/// Elements that belong in `<head>` when a fragment has to be wrapped.
const HEAD_ONLY: &[&str] = &["base", "link", "meta", "script", "style", "title"];

enum Node {
    Text(String),
    Element(Element),
}

struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

impl Element {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            attrs: Vec::new(),
            children: Vec::new(),
        }
    }
}

pub fn pretty_print(html: &str) -> String {
    let nodes = parse(html);
    let root = wrap_document(nodes);
    write_element(&root, 0, false)
}

// ---------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------

fn parse(html: &str) -> Vec<Node> {
    // `stack[0]` is a nameless container for the top-level nodes.
    let mut stack = vec![Element::new("")];
    let mut rest = html;

    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut stack, decode(rest));
            break;
        };
        if lt > 0 {
            push_text(&mut stack, decode(&rest[..lt]));
            rest = &rest[lt..];
        }

        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.split_once("-->").map_or("", |(_, tail)| tail);
        } else if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.split_once('>').map_or("", |(_, tail)| tail);
        } else if let Some(after) = rest.strip_prefix("</") {
            let (tag, tail) = after.split_once('>').unwrap_or((after, ""));
            close(&mut stack, tag.trim().to_ascii_lowercase().as_str());
            rest = tail;
        } else if rest[1..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            let (tag, attrs, self_closing, tail) = read_start_tag(&rest[1..]);
            rest = tail;

            let mut element = Element::new(&tag);
            element.attrs = attrs;
            let is_void = VOID.contains(&tag.as_str());

            if is_void || self_closing {
                push_node(&mut stack, Node::Element(element));
                continue;
            }

            match tag.as_str() {
                "script" | "style" | "textarea" | "title" => {
                    let close_tag = format!("</{tag}");
                    let end = find_ignore_case(rest, &close_tag).unwrap_or(rest.len());
                    let mut body = &rest[..end];
                    if tag == "textarea" {
                        body = body.strip_prefix('\n').unwrap_or(body);
                    }
                    if !body.is_empty() {
                        let text = if matches!(tag.as_str(), "script" | "style") {
                            body.to_string()
                        } else {
                            decode(body)
                        };
                        element.children.push(Node::Text(text));
                    }
                    push_node(&mut stack, Node::Element(element));
                    rest = rest[end..].split_once('>').map_or("", |(_, tail)| tail);
                }
                _ => {
                    if tag == "pre" {
                        rest = rest.strip_prefix('\n').unwrap_or(rest);
                    }
                    stack.push(element);
                }
            }
        } else {
            // A stray `<` that doesn't start a tag is just text.
            push_text(&mut stack, "<".to_string());
            rest = &rest[1..];
        }
    }

    while stack.len() > 1 {
        let done = stack.pop().unwrap();
        push_node(&mut stack, Node::Element(done));
    }
    stack.pop().unwrap().children
}

fn push_node(stack: &mut [Element], node: Node) {
    stack.last_mut().unwrap().children.push(node);
}

fn push_text(stack: &mut [Element], text: String) {
    if text.is_empty() {
        return;
    }
    let parent = stack.last_mut().unwrap();
    if let Some(Node::Text(last)) = parent.children.last_mut() {
        last.push_str(&text);
    } else {
        parent.children.push(Node::Text(text));
    }
}

/// Closes the nearest open element called `name`; an end tag with no
/// matching open element (`</link>` after a void `<link>`) is ignored.
fn close(stack: &mut Vec<Element>, name: &str) {
    let Some(depth) = stack.iter().skip(1).rposition(|e| e.name == name) else {
        return;
    };
    while stack.len() > depth + 1 {
        let done = stack.pop().unwrap();
        push_node(stack, Node::Element(done));
    }
}

type StartTag<'a> = (String, Vec<(String, String)>, bool, &'a str);

/// `input` starts right after the `<`.
fn read_start_tag(input: &str) -> StartTag<'_> {
    let name_end = input
        .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .unwrap_or(input.len());
    let name = input[..name_end].to_ascii_lowercase();
    let mut rest = &input[name_end..];
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut self_closing = false;

    loop {
        rest = rest.trim_start();
        if let Some(tail) = rest.strip_prefix('>') {
            return (name, attrs, self_closing, tail);
        }
        if let Some(tail) = rest.strip_prefix('/') {
            self_closing = tail.trim_start().starts_with('>');
            rest = tail;
            continue;
        }
        if rest.is_empty() {
            return (name, attrs, self_closing, rest);
        }

        let key_end = rest
            .find(|c: char| c.is_ascii_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(rest.len())
            .max(1);
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();

        let mut value = String::new();
        if let Some(tail) = rest.strip_prefix('=') {
            let tail = tail.trim_start();
            if let Some(quote @ ('"' | '\'')) = tail.chars().next() {
                let body = &tail[1..];
                let end = body.find(quote).unwrap_or(body.len());
                value = decode(&body[..end]);
                rest = body.get(end + 1..).unwrap_or("");
            } else {
                let end = tail
                    .find(|c: char| c.is_ascii_whitespace() || c == '>')
                    .unwrap_or(tail.len());
                value = decode(&tail[..end]);
                rest = &tail[end..];
            }
        }
        // First occurrence of a duplicated attribute wins.
        if !attrs.iter().any(|(k, _)| *k == key) {
            attrs.push((key, value));
        }
    }
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

/// Decodes character references: the named ones HTML source realistically
/// contains plus decimal / hex numeric references. Anything else is kept
/// literally, as a browser does for an unknown reference.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest
            .find(';')
            .filter(|&semi| semi <= 10)
            .and_then(|semi| reference(&rest[1..semi]).map(|c| (c, semi)));
        if let Some((c, semi)) = decoded {
            out.push(c);
            rest = &rest[semi + 1..];
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn reference(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some('\u{a0}'),
        _ => {
            let digits = name.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse().ok()?,
            };
            char::from_u32(code)
        }
    }
}

/// Gives the parsed top-level nodes a single `<html>` root with a `<head>`
/// and a `<body>`, moving things into them the way a browser would for a
/// fragment.
fn wrap_document(nodes: Vec<Node>) -> Element {
    let is_html = |n: &Node| matches!(n, Node::Element(e) if e.name == "html");
    if let Some(i) = nodes.iter().position(is_html) {
        let Some(Node::Element(mut html)) = nodes.into_iter().nth(i) else {
            unreachable!()
        };
        let has = |html: &Element, name: &str| {
            html.children
                .iter()
                .any(|n| matches!(n, Node::Element(e) if e.name == name))
        };
        if !has(&html, "head") {
            html.children.insert(0, Node::Element(Element::new("head")));
        }
        if !has(&html, "body") {
            html.children.push(Node::Element(Element::new("body")));
        }
        return html;
    }

    let mut head = Element::new("head");
    let mut body = Element::new("body");
    let mut in_head = true;
    for node in nodes {
        match &node {
            Node::Text(t) if in_head && t.trim().is_empty() => {}
            Node::Element(e) if in_head && HEAD_ONLY.contains(&e.name.as_str()) => {
                head.children.push(node);
            }
            _ => {
                in_head = false;
                body.children.push(node);
            }
        }
    }
    let mut html = Element::new("html");
    html.children.push(Node::Element(head));
    html.children.push(Node::Element(body));
    html
}

// ---------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------

/// `is_preformatted` keeps whitespace exactly (`pre`, `code`, `textarea`,
/// ...). `raw_text` (see `write_node`) is stricter and rarer: the body of
/// `script` and `style` is code, not text, and must not be escaped. Every other text node *is*
/// escaped here - the reader decoded `&lt;` back to `<` on the way in, so
/// writing it out as-is would turn any text a page displayed (a username, a
/// title) into live markup.
fn write_element(e: &Element, indent: usize, is_preformatted: bool) -> String {
    let attrs = e.attrs.iter().fold(String::new(), |mut out, (k, v)| {
        let _ = write!(out, " {k}=\"{}\"", html_escape(v));
        out
    });

    let tag = e.name.as_str();
    let code = tag == "script" || tag == "style";
    let pre = code || tag == "pre" || tag == "code" || tag == "textarea";

    if pre {
        let mut s = format!("<{tag}{attrs}>");
        for child in &e.children {
            s.push_str(&write_node(child, 0, true, code));
        }
        let _ = write!(s, "</{tag}>");
        if is_preformatted {
            s
        } else {
            format!("{}{s}\n", " ".repeat(indent))
        }
    } else {
        let mut s = format!("{}<{tag}{attrs}>\n", " ".repeat(indent));
        for child in &e.children {
            s.push_str(&write_node(child, indent + 4, false, false));
        }
        let _ = writeln!(s, "{}</{tag}>", " ".repeat(indent));
        s
    }
}

fn write_node(node: &Node, indent: usize, is_preformatted: bool, raw_text: bool) -> String {
    match node {
        Node::Element(e) => write_element(e, indent, is_preformatted),
        Node::Text(text) => {
            if raw_text {
                text.clone()
            } else if is_preformatted {
                html_escape(text)
            } else {
                let text = text.trim();
                if text.is_empty() {
                    String::new()
                } else {
                    format!("{}{}\n", " ".repeat(indent), html_escape(text))
                }
            }
        }
    }
}
