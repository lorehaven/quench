//! A tolerant HTML parser and the document tree it builds.
//!
//! The parser follows the shape of HTML5 parsing without implementing the whole algorithm: it always
//! produces `html`, `head` and `body` elements, puts head content in `head` and the rest in `body`, closes
//! elements that HTML closes implicitly (`p`, `li`, `td`, ...), ignores stray end tags, treats `script` and
//! `style` as raw text and `title` and `textarea` as text, and decodes character references. Nothing in the
//! input makes it fail.

use crate::selector::Selector;

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Document,
    Element,
    Text,
    Comment,
    Doctype,
}

#[derive(Debug)]
enum Data {
    Document,
    Element {
        name: String,
        attrs: Vec<(String, String)>,
    },
    Text(String),
    Comment(String),
    Doctype(String),
}

#[derive(Debug)]
struct Entry {
    data: Data,
    parent: Option<usize>,
    children: Vec<usize>,
}

/// A parsed HTML document.
#[derive(Debug)]
pub struct Document {
    nodes: Vec<Entry>,
}

/// A node of a [`Document`]; cheap to copy.
#[derive(Debug, Clone, Copy)]
pub struct Node<'a> {
    document: &'a Document,
    id: usize,
}

impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.document, other.document) && self.id == other.id
    }
}

impl Eq for Node<'_> {}

impl Document {
    /// Parses HTML. Never fails.
    #[must_use]
    pub fn parse(html: &str) -> Self {
        Builder::run(html.strip_prefix('\u{FEFF}').unwrap_or(html))
    }

    /// The document node, parent of the `html` element.
    #[must_use]
    pub const fn root(&self) -> Node<'_> {
        Node {
            document: self,
            id: 0,
        }
    }

    /// The `html` element.
    ///
    /// # Panics
    ///
    /// Never: the parser always creates it.
    #[must_use]
    pub fn root_element(&self) -> Node<'_> {
        self.root()
            .children()
            .find(Node::is_element)
            .expect("the parser always creates an html element")
    }

    /// The elements matching `selector`, in document order.
    pub fn select<'a, 's>(
        &'a self,
        selector: &'s Selector,
    ) -> impl Iterator<Item = Node<'a>> + use<'a, 's> {
        self.root().select(selector)
    }
}

impl<'a> Node<'a> {
    fn entry(&self) -> &'a Entry {
        &self.document.nodes[self.id]
    }

    const fn at(&self, id: usize) -> Self {
        Node {
            document: self.document,
            id,
        }
    }

    #[must_use]
    pub fn kind(&self) -> NodeKind {
        match self.entry().data {
            Data::Document => NodeKind::Document,
            Data::Element { .. } => NodeKind::Element,
            Data::Text(_) => NodeKind::Text,
            Data::Comment(_) => NodeKind::Comment,
            Data::Doctype(_) => NodeKind::Doctype,
        }
    }

    #[must_use]
    pub fn is_element(&self) -> bool {
        self.kind() == NodeKind::Element
    }

    #[must_use]
    pub fn is_document(&self) -> bool {
        self.kind() == NodeKind::Document
    }

    #[must_use]
    pub fn is_comment(&self) -> bool {
        self.kind() == NodeKind::Comment
    }

    /// The tag name of an element, in lower case.
    #[must_use]
    pub fn name(&self) -> Option<&'a str> {
        match &self.entry().data {
            Data::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    /// An attribute of an element (names are lower case).
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&'a str> {
        match &self.entry().data {
            Data::Element { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }

    /// All attributes of an element, in source order.
    pub fn attrs(self) -> impl Iterator<Item = (&'a str, &'a str)> {
        let list: &'a [(String, String)] = match &self.entry().data {
            Data::Element { attrs, .. } => attrs,
            _ => &[],
        };
        list.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    #[must_use]
    pub fn id(&self) -> Option<&'a str> {
        self.attr("id")
    }

    /// The classes in the `class` attribute.
    pub fn classes(self) -> impl Iterator<Item = &'a str> {
        self.attr("class").unwrap_or("").split_whitespace()
    }

    #[must_use]
    pub fn has_class(&self, class: &str) -> bool {
        self.classes().any(|c| c == class)
    }

    /// The content of a text node.
    #[must_use]
    pub fn as_text(&self) -> Option<&'a str> {
        match &self.entry().data {
            Data::Text(t) => Some(t),
            _ => None,
        }
    }

    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.entry().parent.map(|p| self.at(p))
    }

    #[must_use]
    pub fn children(self) -> impl DoubleEndedIterator<Item = Node<'a>> + 'a {
        let node = self;
        self.entry().children.iter().map(move |&c| node.at(c))
    }

    /// The parents, nearest first (the document node last).
    pub fn ancestors(self) -> impl Iterator<Item = Node<'a>> {
        std::iter::successors(self.parent(), Node::parent)
    }

    /// This node and everything below it, in document order.
    pub fn descendants(self) -> impl Iterator<Item = Node<'a>> {
        let mut stack = vec![self.id];
        let node = self;
        std::iter::from_fn(move || {
            let id = stack.pop()?;
            stack.extend(node.document.nodes[id].children.iter().rev());
            Some(node.at(id))
        })
    }

    /// The elements below this node (not itself) that match `selector`, in document order.
    pub fn select<'s>(
        self,
        selector: &'s Selector,
    ) -> impl Iterator<Item = Node<'a>> + use<'a, 's> {
        let root = self.id;
        self.descendants()
            .filter(move |n| n.id != root && selector.matches(*n))
    }

    /// The text nodes below this node, in order.
    pub fn text_nodes(self) -> impl Iterator<Item = &'a str> {
        self.descendants().filter_map(|n| n.as_text())
    }

    /// All the text below this node, concatenated.
    #[must_use]
    pub fn text(&self) -> String {
        self.text_nodes().collect()
    }

    /// The previous sibling that is an element.
    #[must_use]
    pub fn prev_element(&self) -> Option<Self> {
        let parent = self.parent()?;
        let siblings = &parent.entry().children;
        let at = siblings.iter().position(|&c| c == self.id)?;
        siblings[..at]
            .iter()
            .rev()
            .map(|&c| self.at(c))
            .find(Node::is_element)
    }

    /// The next sibling that is an element.
    #[must_use]
    pub fn next_element(&self) -> Option<Self> {
        let parent = self.parent()?;
        let siblings = &parent.entry().children;
        let at = siblings.iter().position(|&c| c == self.id)?;
        siblings[at + 1..]
            .iter()
            .map(|&c| self.at(c))
            .find(Node::is_element)
    }

    /// The node serialised back to HTML (text and attributes escaped, void elements without end tags).
    #[must_use]
    pub fn html(&self) -> String {
        let mut out = String::new();
        self.write_html(&mut out);
        out
    }

    /// The children serialised back to HTML (the node's own tag left out).
    #[must_use]
    pub fn inner_html(&self) -> String {
        let mut out = String::new();
        self.children().for_each(|c| c.write_html(&mut out));
        out
    }

    fn write_html(&self, out: &mut String) {
        match &self.entry().data {
            Data::Document => self.children().for_each(|c| c.write_html(out)),
            Data::Text(t) => {
                let raw = self
                    .parent()
                    .and_then(|p| p.name())
                    .is_some_and(|n| matches!(n, "script" | "style"));
                if raw {
                    out.push_str(t);
                } else {
                    escape(t, false, out);
                }
            }
            Data::Comment(c) => {
                out.push_str("<!--");
                out.push_str(c);
                out.push_str("-->");
            }
            Data::Doctype(d) => {
                out.push_str("<!DOCTYPE ");
                out.push_str(d);
                out.push('>');
            }
            Data::Element { name, attrs } => {
                out.push('<');
                out.push_str(name);
                for (k, v) in attrs {
                    out.push(' ');
                    out.push_str(k);
                    out.push_str("=\"");
                    escape(v, true, out);
                    out.push('"');
                }
                out.push('>');
                if !VOID.contains(&name.as_str()) {
                    self.children().for_each(|c| c.write_html(out));
                    out.push_str("</");
                    out.push_str(name);
                    out.push('>');
                }
            }
        }
    }
}

fn escape(text: &str, attribute: bool, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];
const HEAD_ONLY: [&str; 7] = [
    "title", "meta", "link", "base", "style", "script", "template",
];
/// Start tags that close an open `p`.
const CLOSES_P: [&str; 36] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "ul",
    "li",
    "dd",
];
/// Elements an implicit end tag does not look past.
const SCOPE: [&str; 9] = [
    "html", "table", "td", "th", "caption", "template", "button", "object", "marquee",
];

struct Builder {
    nodes: Vec<Entry>,
    stack: Vec<usize>,
    html: Option<usize>,
    head: Option<usize>,
    body: Option<usize>,
}

impl Builder {
    fn run(input: &str) -> Document {
        let mut b = Self {
            nodes: vec![Entry {
                data: Data::Document,
                parent: None,
                children: Vec::new(),
            }],
            stack: vec![0],
            html: None,
            head: None,
            body: None,
        };
        b.tokenize(input);
        b.ensure_body();
        Document { nodes: b.nodes }
    }

    fn add(&mut self, parent: usize, data: Data) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Entry {
            data,
            parent: Some(parent),
            children: Vec::new(),
        });
        self.nodes[parent].children.push(id);
        id
    }

    fn current(&self) -> usize {
        *self.stack.last().unwrap_or(&0)
    }

    fn name_of(&self, id: usize) -> &str {
        match &self.nodes[id].data {
            Data::Element { name, .. } => name,
            _ => "",
        }
    }

    fn ensure_html(&mut self) -> usize {
        if let Some(h) = self.html {
            return h;
        }
        let id = self.add(
            0,
            Data::Element {
                name: "html".into(),
                attrs: Vec::new(),
            },
        );
        self.html = Some(id);
        self.stack = vec![0, id];
        id
    }

    fn ensure_head(&mut self) -> usize {
        if let Some(h) = self.head {
            return h;
        }
        let html = self.ensure_html();
        let id = self.add(
            html,
            Data::Element {
                name: "head".into(),
                attrs: Vec::new(),
            },
        );
        self.head = Some(id);
        self.stack.push(id);
        id
    }

    fn ensure_body(&mut self) -> usize {
        if let Some(b) = self.body {
            return b;
        }
        let html = self.ensure_html();
        self.ensure_head();
        // The head is closed once the body begins.
        self.stack.retain(|&e| Some(e) != self.head);
        let id = self.add(
            html,
            Data::Element {
                name: "body".into(),
                attrs: Vec::new(),
            },
        );
        self.body = Some(id);
        self.stack.push(id);
        id
    }

    fn insert_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.body.is_none() && matches!(self.name_of(self.current()), "" | "html" | "head") {
            // Whitespace before the head is dropped, inside it kept; any other text starts the body.
            let rest = text.trim_start();
            if self.name_of(self.current()) == "head" && rest.len() < text.len() {
                self.add_text(&text[..text.len() - rest.len()]);
            }
            if rest.is_empty() {
                return;
            }
            self.ensure_body();
            self.add_text(rest);
            return;
        }
        self.add_text(text);
    }

    fn add_text(&mut self, text: &str) {
        let parent = self.current();
        if let Some(&last) = self.nodes[parent].children.last()
            && let Data::Text(existing) = &mut self.nodes[last].data
        {
            existing.push_str(text);
            return;
        }
        self.add(parent, Data::Text(text.to_string()));
    }

    fn in_scope(&self, name: &str, stop: &[&str]) -> Option<usize> {
        for (i, &id) in self.stack.iter().enumerate().rev() {
            let n = self.name_of(id);
            if n == name {
                return Some(i);
            }
            if stop.contains(&n) {
                return None;
            }
        }
        None
    }

    fn pop_to(&mut self, index: usize) {
        self.stack.truncate(index);
    }

    fn start_tag(&mut self, name: &str, attrs: Vec<(String, String)>, self_closing: bool) {
        match name {
            "html" => {
                let id = self.ensure_html();
                if let Data::Element {
                    attrs: existing, ..
                } = &mut self.nodes[id].data
                {
                    for (k, v) in attrs {
                        if !existing.iter().any(|(ek, _)| *ek == k) {
                            existing.push((k, v));
                        }
                    }
                }
                return;
            }
            "head" => {
                if self.head.is_none() && self.body.is_none() {
                    let id = self.ensure_head();
                    if let Data::Element {
                        attrs: existing, ..
                    } = &mut self.nodes[id].data
                    {
                        *existing = attrs;
                    }
                }
                return;
            }
            "body" => {
                let existed = self.body.is_some();
                let id = self.ensure_body();
                if !existed
                    && let Data::Element {
                        attrs: existing, ..
                    } = &mut self.nodes[id].data
                {
                    *existing = attrs;
                }
                return;
            }
            _ => {}
        }
        if self.body.is_none() {
            if HEAD_ONLY.contains(&name) {
                self.ensure_head();
            } else {
                self.ensure_body();
            }
        }
        self.close_implied(name);
        match name {
            "tr" if self.name_of(self.current()) == "table" => self.open_implied("tbody"),
            "td" | "th" => {
                if self.name_of(self.current()) == "table" {
                    self.open_implied("tbody");
                }
                if matches!(self.name_of(self.current()), "tbody" | "thead" | "tfoot") {
                    self.open_implied("tr");
                }
            }
            _ => {}
        }
        let parent = self.current();
        let id = self.add(
            parent,
            Data::Element {
                name: name.to_string(),
                attrs,
            },
        );
        if !VOID.contains(&name) && !self_closing_void(name, self_closing) {
            self.stack.push(id);
        }
    }

    fn open_implied(&mut self, name: &str) {
        let parent = self.current();
        let id = self.add(
            parent,
            Data::Element {
                name: name.to_string(),
                attrs: Vec::new(),
            },
        );
        self.stack.push(id);
    }

    /// Closes the elements that HTML ends implicitly when a `name` start tag appears.
    fn close_implied(&mut self, name: &str) {
        if CLOSES_P.contains(&name)
            && let Some(i) = self.in_scope("p", &SCOPE)
        {
            self.pop_to(i);
        }
        match name {
            "li" => {
                if let Some(i) =
                    self.in_scope("li", &["ul", "ol", "table", "td", "th", "template", "html"])
                {
                    self.pop_to(i);
                }
            }
            "dd" | "dt" => {
                for target in ["dd", "dt"] {
                    if let Some(i) =
                        self.in_scope(target, &["dl", "table", "td", "th", "template", "html"])
                    {
                        self.pop_to(i);
                        break;
                    }
                }
            }
            "option" => {
                if let Some(i) = self.in_scope("option", &["select", "datalist", "html"]) {
                    self.pop_to(i);
                }
            }
            "optgroup" => {
                for target in ["option", "optgroup"] {
                    if let Some(i) = self.in_scope(target, &["select", "html"]) {
                        self.pop_to(i);
                        break;
                    }
                }
            }
            "tr" => {
                if let Some(i) = self.in_scope("tr", &["table", "html"]) {
                    self.pop_to(i);
                }
            }
            "td" | "th" => {
                let cell = ["td", "th"]
                    .iter()
                    .filter_map(|t| self.in_scope(t, &["tr", "table", "html"]))
                    .max();
                if let Some(i) = cell {
                    self.pop_to(i);
                }
            }
            "tbody" | "thead" | "tfoot" => {
                let section = ["tbody", "thead", "tfoot"]
                    .iter()
                    .filter_map(|t| self.in_scope(t, &["table", "html"]))
                    .max();
                if let Some(i) = section {
                    self.pop_to(i);
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                if matches!(
                    self.name_of(self.current()),
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
                ) {
                    self.stack.pop();
                }
            }
            "a" => {
                if let Some(i) = self.in_scope("a", &SCOPE) {
                    self.pop_to(i);
                }
            }
            "button" => {
                if let Some(i) = self.in_scope("button", &["html", "table"]) {
                    self.pop_to(i);
                }
            }
            _ => {}
        }
    }

    fn end_tag(&mut self, name: &str) {
        match name {
            "html" | "body" => return,
            "head" => {
                if let Some(i) = self.in_scope("head", &[]) {
                    self.pop_to(i);
                }
                return;
            }
            "p" if self.in_scope("p", &SCOPE).is_none() => {
                // A stray `</p>` makes an empty paragraph.
                self.start_tag("p", Vec::new(), false);
                self.stack.pop();
                return;
            }
            "br" => {
                self.start_tag("br", Vec::new(), false);
                return;
            }
            _ => {}
        }
        let stop: &[&str] = if matches!(
            name,
            "td" | "th" | "tr" | "table" | "tbody" | "thead" | "tfoot" | "caption"
        ) {
            &["html", "template"]
        } else {
            &SCOPE
        };
        if let Some(i) = self.in_scope(name, stop) {
            self.pop_to(i);
        }
    }

    fn tokenize(&mut self, input: &str) {
        let bytes = input.as_bytes();
        let mut pos = 0;
        let mut text = String::new();
        while pos < bytes.len() {
            let Some(lt) = input[pos..].find('<') else {
                text.push_str(&input[pos..]);
                break;
            };
            text.push_str(&input[pos..pos + lt]);
            pos += lt;
            let rest = &input[pos..];
            let next = rest.as_bytes().get(1).copied();
            if let Some(after) = rest.strip_prefix("<!--") {
                self.flush_text(&mut text);
                let (body, used) = match after.find("-->") {
                    Some(i) => (&after[..i], 4 + i + 3),
                    None => (after, rest.len()),
                };
                let body = if rest.starts_with("<!-->") { "" } else { body };
                let used = if rest.starts_with("<!-->") { 5 } else { used };
                let parent = self.comment_parent();
                self.add(parent, Data::Comment(body.to_string()));
                pos += used;
            } else if rest.starts_with("<!") || rest.starts_with("<?") {
                self.flush_text(&mut text);
                let end = rest.find('>').map_or(rest.len(), |i| i + 1);
                let inner = &rest[2..end.saturating_sub(1).max(2)];
                if rest[2..].to_ascii_lowercase().starts_with("doctype") && self.html.is_none() {
                    self.add(
                        0,
                        Data::Doctype(inner[7.min(inner.len())..].trim().to_string()),
                    );
                } else {
                    let parent = self.comment_parent();
                    self.add(parent, Data::Comment(inner.to_string()));
                }
                pos += end;
            } else if next == Some(b'/')
                && rest.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic)
            {
                self.flush_text(&mut text);
                let end = rest.find('>').map_or(rest.len(), |i| i + 1);
                let name = rest[2..]
                    .bytes()
                    .take_while(|b| !b.is_ascii_whitespace() && *b != b'>' && *b != b'/')
                    .count();
                let name = rest[2..2 + name].to_ascii_lowercase();
                self.end_tag(&name);
                pos += end;
            } else if next.is_some_and(|b| b.is_ascii_alphabetic()) {
                self.flush_text(&mut text);
                let (name, attrs, self_closing, used) = parse_start_tag(rest);
                pos += used;
                self.start_tag(&name, attrs, self_closing);
                if matches!(name.as_str(), "pre" | "listing" | "textarea")
                    && input[pos..].starts_with('\n')
                {
                    pos += 1;
                }
                if matches!(
                    name.as_str(),
                    "script"
                        | "style"
                        | "textarea"
                        | "title"
                        | "xmp"
                        | "iframe"
                        | "noembed"
                        | "noframes"
                        | "noscript"
                ) && !self_closing
                {
                    // Raw text up to the matching end tag.
                    let lower = input[pos..].to_ascii_lowercase();
                    let close = format!("</{name}");
                    let end = lower.find(&close).unwrap_or(input.len() - pos);
                    let raw = &input[pos..pos + end];
                    if matches!(name.as_str(), "textarea" | "title") {
                        self.insert_text(&decode_text(raw));
                    } else {
                        self.insert_text(raw);
                    }
                    pos += end;
                }
            } else {
                text.push('<');
                pos += 1;
            }
        }
        self.flush_text(&mut text);
    }

    /// Comments before the `html` element hang off the document.
    fn comment_parent(&self) -> usize {
        if self.html.is_none() {
            0
        } else {
            self.current()
        }
    }

    fn flush_text(&mut self, text: &mut String) {
        if text.is_empty() {
            return;
        }
        let decoded = decode_text(text);
        text.clear();
        self.insert_text(&decoded);
    }
}

const fn self_closing_void(_name: &str, _self_closing: bool) -> bool {
    // HTML ignores the `/` on non-void elements, so `<div/>` stays open.
    false
}

fn decode_text(text: &str) -> String {
    crate::entities::decode(text, false)
}

/// Parses `<name attr=value ...>` at the start of `rest`: name, attributes, whether it ended with `/>`,
/// and how many bytes it took.
fn parse_start_tag(rest: &str) -> (String, Vec<(String, String)>, bool, usize) {
    let bytes = rest.as_bytes();
    let mut i = 1;
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' && bytes[i] != b'>'
    {
        i += 1;
    }
    let name = rest[1..i].to_ascii_lowercase();
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut self_closing = false;
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'>') {
                self_closing = true;
            }
            i += 1;
        }
        if i >= bytes.len() {
            return (name, attrs, self_closing, bytes.len());
        }
        if bytes[i] == b'>' {
            return (name, attrs, self_closing, i + 1);
        }
        let start = i;
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && !matches!(bytes[i], b'=' | b'>' | b'/')
        {
            i += 1;
        }
        if i == start {
            // A stray `=`: skip it.
            i += 1;
            continue;
        }
        let key = rest[start..i].to_ascii_lowercase();
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if bytes.get(i) == Some(&b'=') {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            match bytes.get(i) {
                Some(&q @ (b'"' | b'\'')) => {
                    let end = rest[i + 1..]
                        .find(q as char)
                        .map_or(rest.len(), |e| i + 1 + e);
                    value = decode_attribute(&rest[i + 1..end]);
                    i = (end + 1).min(rest.len());
                }
                Some(_) => {
                    let start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    value = decode_attribute(&rest[start..i]);
                }
                None => {}
            }
        }
        if !attrs.iter().any(|(k, _)| *k == key) {
            attrs.push((key, value));
        }
    }
}

/// Character references in an attribute value: a legacy name without `;` is left alone when it is followed
/// by `=` or a letter or digit.
fn decode_attribute(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::new();
    let mut rest = value;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some((text, used)) = crate::entities::reference(rest, false) {
            let bare = !rest[..used].ends_with(';');
            let next = rest[used..].chars().next();
            if bare
                && next.is_some_and(|c| c == '=' || c.is_ascii_alphanumeric())
                && !rest.starts_with("&#")
            {
                out.push('&');
                rest = &rest[1..];
            } else {
                out.push_str(&text);
                rest = &rest[used..];
            }
        } else {
            out.push('&');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}
