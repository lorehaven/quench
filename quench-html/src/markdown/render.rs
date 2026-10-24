//! The HTML renderer for the parsed blocks and inlines.

use super::blocks::{Document, Kind};
use super::inlines::{self, Inline};
use super::scan::{encode_uri, escape_attr, escape_text};

struct Out(String);

impl Out {
    /// Starts a new line unless already at one.
    fn cr(&mut self) {
        if !self.0.is_empty() && !self.0.ends_with('\n') {
            self.0.push('\n');
        }
    }
}

pub fn render(document: &Document) -> String {
    let mut out = Out(String::new());
    for &child in &document.nodes[0].children {
        block(document, child, &mut out);
    }
    out.0
}

fn block(document: &Document, id: usize, out: &mut Out) {
    let node = &document.nodes[id];
    match node.kind {
        Kind::Document => {}
        Kind::BlockQuote => {
            out.cr();
            out.0.push_str("<blockquote>\n");
            for &c in &node.children {
                block(document, c, out);
            }
            out.cr();
            out.0.push_str("</blockquote>\n");
        }
        Kind::List => {
            out.cr();
            if node.list.ordered {
                if node.list.start == 1 {
                    out.0.push_str("<ol>\n");
                } else {
                    out.0
                        .push_str(&format!("<ol start=\"{}\">\n", node.list.start));
                }
            } else {
                out.0.push_str("<ul>\n");
            }
            for &c in &node.children {
                block(document, c, out);
            }
            out.cr();
            out.0.push_str(if node.list.ordered {
                "</ol>\n"
            } else {
                "</ul>\n"
            });
        }
        Kind::Item => {
            out.0.push_str("<li>");
            for &c in &node.children {
                block(document, c, out);
            }
            out.0.push_str("</li>\n");
        }
        Kind::Paragraph => {
            let tight = node
                .parent
                .and_then(|p| document.nodes[p].parent)
                .is_some_and(|g| {
                    document.nodes[g].kind == Kind::List && document.nodes[g].list.tight
                });
            let content = inlines::parse(&node.content, &document.references);
            if tight {
                inline_list(&content, out);
            } else {
                out.cr();
                out.0.push_str("<p>");
                inline_list(&content, out);
                out.0.push_str("</p>\n");
            }
        }
        Kind::Heading => {
            out.cr();
            out.0.push_str(&format!("<h{}>", node.level));
            let content = inlines::parse(&node.content, &document.references);
            inline_list(&content, out);
            out.0.push_str(&format!("</h{}>\n", node.level));
        }
        Kind::ThematicBreak => {
            out.cr();
            out.0.push_str("<hr />\n");
        }
        Kind::CodeBlock => {
            out.cr();
            let word = node.info.split_whitespace().next().unwrap_or("");
            if word.is_empty() {
                out.0.push_str("<pre><code>");
            } else {
                out.0.push_str("<pre><code class=\"language-");
                escape_attr(word, &mut out.0);
                out.0.push_str("\">");
            }
            escape_text(&node.literal, &mut out.0);
            out.0.push_str("</code></pre>\n");
        }
        Kind::HtmlBlock => {
            out.cr();
            out.0.push_str(&node.literal);
            out.cr();
        }
    }
}

fn inline_list(list: &[Inline], out: &mut Out) {
    for inline in list {
        inline_one(inline, out);
    }
}

fn inline_one(inline: &Inline, out: &mut Out) {
    match inline {
        Inline::Text(t) => escape_text(t, &mut out.0),
        Inline::Code(c) => {
            out.0.push_str("<code>");
            escape_text(c, &mut out.0);
            out.0.push_str("</code>");
        }
        Inline::Emph(children) => {
            out.0.push_str("<em>");
            inline_list(children, out);
            out.0.push_str("</em>");
        }
        Inline::Strong(children) => {
            out.0.push_str("<strong>");
            inline_list(children, out);
            out.0.push_str("</strong>");
        }
        Inline::Link {
            destination,
            title,
            children,
        } => {
            out.0.push_str("<a href=\"");
            escape_attr(&encode_uri(destination), &mut out.0);
            out.0.push('"');
            if !title.is_empty() {
                out.0.push_str(" title=\"");
                escape_attr(title, &mut out.0);
                out.0.push('"');
            }
            out.0.push('>');
            inline_list(children, out);
            out.0.push_str("</a>");
        }
        Inline::Image {
            destination,
            title,
            children,
        } => {
            out.0.push_str("<img src=\"");
            escape_attr(&encode_uri(destination), &mut out.0);
            out.0.push_str("\" alt=\"");
            let mut alt = String::new();
            plain_text(children, &mut alt);
            escape_attr(&alt, &mut out.0);
            out.0.push('"');
            if !title.is_empty() {
                out.0.push_str(" title=\"");
                escape_attr(title, &mut out.0);
                out.0.push('"');
            }
            out.0.push_str(" />");
        }
        Inline::Html(h) => out.0.push_str(h),
        Inline::SoftBreak => out.0.push('\n'),
        Inline::HardBreak => out.0.push_str("<br />\n"),
    }
}

/// The text of inlines without any markup (an image's alt text).
fn plain_text(list: &[Inline], out: &mut String) {
    for inline in list {
        match inline {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Emph(c) | Inline::Strong(c) => plain_text(c, out),
            Inline::Link { children, .. } | Inline::Image { children, .. } => {
                plain_text(children, out);
            }
            Inline::SoftBreak | Inline::HardBreak => out.push('\n'),
            Inline::Html(_) => {}
        }
    }
}
