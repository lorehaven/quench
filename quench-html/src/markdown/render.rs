//! The HTML renderer of a parsed [`Document`]. The extensions are written the way cmark-gfm writes them
//! (tables, `<del>`, task list checkboxes, the footnotes section), so the output matches GitHub's; math is
//! written as `<span class="math inline">\(…\)</span>` and `<div class="math display">\[…\]</div>`, the TeX
//! escaped, for the auto-render of `KaTeX` or `MathJax` to typeset.

use super::ast::{Alignment, Block, Document, Footnote, Inline, List, Table};
use super::scan::{encode_uri, escape_attr, escape_text};

struct Out(String);

impl Out {
    /// Starts a new line unless already at one.
    fn cr(&mut self) {
        if !self.0.is_empty() && !self.0.ends_with('\n') {
            self.0.push('\n');
        }
    }

    fn push(&mut self, s: &str) {
        self.0.push_str(s);
    }
}

/// `document` as HTML.
#[must_use]
pub fn render(document: &Document) -> String {
    let mut out = Out(String::new());
    for b in &document.blocks {
        block(b, false, &mut out);
    }
    if !document.footnotes.is_empty() {
        footnotes(&document.footnotes, &mut out);
    }
    out.0
}

/// One block; `tight` when it is directly in an item of a tight list (a paragraph there is not wrapped).
fn block(b: &Block, tight: bool, out: &mut Out) {
    match b {
        Block::Paragraph(content) => paragraph(content, tight, "", out),
        Block::Heading { level, content } => {
            out.cr();
            out.push(&format!("<h{level}>"));
            inlines(content, out);
            out.push(&format!("</h{level}>\n"));
        }
        Block::ThematicBreak => {
            out.cr();
            out.push("<hr />\n");
        }
        Block::CodeBlock { info, literal, .. } => {
            out.cr();
            let word = info.split_whitespace().next().unwrap_or("");
            if word.is_empty() {
                out.push("<pre><code>");
            } else {
                out.push("<pre><code class=\"language-");
                escape_attr(word, &mut out.0);
                out.push("\">");
            }
            escape_text(literal, &mut out.0);
            out.push("</code></pre>\n");
        }
        Block::Html(html) => {
            out.cr();
            out.push(html);
            out.cr();
        }
        Block::BlockQuote(children) => {
            out.cr();
            out.push("<blockquote>\n");
            for c in children {
                block(c, false, out);
            }
            out.cr();
            out.push("</blockquote>\n");
        }
        Block::List(list) => self::list(list, out),
        Block::Table(table) => self::table(table, out),
        Block::Math(tex) => {
            out.cr();
            out.push("<div class=\"math display\">\\[");
            escape_text(tex, &mut out.0);
            out.push("\\]</div>\n");
        }
    }
}

/// A paragraph, with `extra` (HTML) before its end: a footnote's back references.
fn paragraph(content: &[Inline], tight: bool, extra: &str, out: &mut Out) {
    if tight {
        inlines(content, out);
        out.push(extra);
    } else {
        out.cr();
        out.push("<p>");
        inlines(content, out);
        out.push(extra);
        out.push("</p>\n");
    }
}

fn list(list: &List, out: &mut Out) {
    out.cr();
    if list.ordered {
        if list.start == 1 {
            out.push("<ol>\n");
        } else {
            out.push(&format!("<ol start=\"{}\">\n", list.start));
        }
    } else {
        out.push("<ul>\n");
    }
    for item in &list.items {
        out.push("<li>");
        for (n, b) in item.blocks.iter().enumerate() {
            if n == 0
                && let Some(checked) = item.task
                && let Block::Paragraph(content) = b
            {
                let checkbox = if checked {
                    "<input type=\"checkbox\" checked=\"\" disabled=\"\" /> "
                } else {
                    "<input type=\"checkbox\" disabled=\"\" /> "
                };
                if list.tight {
                    out.push(checkbox);
                    inlines(content, out);
                } else {
                    out.cr();
                    out.push("<p>");
                    out.push(checkbox);
                    inlines(content, out);
                    out.push("</p>\n");
                }
                continue;
            }
            block(b, list.tight, out);
        }
        out.push("</li>\n");
    }
    out.cr();
    out.push(if list.ordered { "</ol>\n" } else { "</ul>\n" });
}

fn table(table: &Table, out: &mut Out) {
    let cell = |tag: &str, align: Alignment, content: &[Inline], out: &mut Out| {
        out.push(&format!("<{tag}"));
        match align {
            Alignment::None => {}
            Alignment::Left => out.push(" align=\"left\""),
            Alignment::Center => out.push(" align=\"center\""),
            Alignment::Right => out.push(" align=\"right\""),
        }
        out.push(">");
        inlines(content, out);
        out.push(&format!("</{tag}>\n"));
    };
    out.cr();
    out.push("<table>\n<thead>\n<tr>\n");
    for (content, &align) in table.header.iter().zip(&table.alignments) {
        cell("th", align, content, out);
    }
    out.push("</tr>\n</thead>\n");
    if !table.rows.is_empty() {
        out.push("<tbody>\n");
        for row in &table.rows {
            out.push("<tr>\n");
            for (content, &align) in row.iter().zip(&table.alignments) {
                cell("td", align, content, out);
            }
            out.push("</tr>\n");
        }
        out.push("</tbody>\n");
    }
    out.push("</table>\n");
}

fn footnotes(list: &[Footnote], out: &mut Out) {
    out.cr();
    out.push("<section class=\"footnotes\" data-footnotes>\n<ol>\n");
    for footnote in list {
        let label = encode_uri(&footnote.label);
        out.push("<li id=\"fn-");
        escape_attr(&label, &mut out.0);
        out.push("\">\n");
        let backrefs = (1..=footnote.references)
            .map(|k| backref(&label, footnote.number, k))
            .collect::<Vec<_>>()
            .join(" ");
        let last = footnote.blocks.len().checked_sub(1);
        for (n, b) in footnote.blocks.iter().enumerate() {
            match b {
                Block::Paragraph(content) if Some(n) == last => {
                    paragraph(content, false, &format!(" {backrefs}"), out);
                }
                _ => block(b, false, out),
            }
        }
        if !matches!(footnote.blocks.last(), Some(Block::Paragraph(_))) {
            out.cr();
            out.push(&backrefs);
            out.push("\n");
        }
        out.push("</li>\n");
    }
    out.push("</ol>\n</section>\n");
}

/// The link from a footnote back to its `k`th reference.
fn backref(label: &str, number: usize, k: usize) -> String {
    let mut out = String::new();
    let (suffix, idx) = if k == 1 {
        (String::new(), number.to_string())
    } else {
        (format!("-{k}"), format!("{number}-{k}"))
    };
    out.push_str("<a href=\"#fnref-");
    escape_attr(label, &mut out);
    out.push_str(&suffix);
    out.push_str(&format!(
        "\" class=\"footnote-backref\" data-footnote-backref data-footnote-backref-idx=\"{idx}\" aria-label=\"Back to reference {idx}\">↩"
    ));
    if k > 1 {
        out.push_str(&format!("<sup class=\"footnote-ref\">{k}</sup>"));
    }
    out.push_str("</a>");
    out
}

fn inlines(list: &[Inline], out: &mut Out) {
    for inline in list {
        inline_one(inline, out);
    }
}

fn inline_one(inline: &Inline, out: &mut Out) {
    match inline {
        Inline::Text(t) => escape_text(t, &mut out.0),
        Inline::Code(c) => {
            out.push("<code>");
            escape_text(c, &mut out.0);
            out.push("</code>");
        }
        Inline::Emph(children) => wrap("em", children, out),
        Inline::Strong(children) => wrap("strong", children, out),
        Inline::Strikethrough(children) => wrap("del", children, out),
        Inline::Link {
            destination,
            title,
            children,
        } => {
            out.push("<a href=\"");
            escape_attr(&encode_uri(destination), &mut out.0);
            out.push("\"");
            if !title.is_empty() {
                out.push(" title=\"");
                escape_attr(title, &mut out.0);
                out.push("\"");
            }
            out.push(">");
            inlines(children, out);
            out.push("</a>");
        }
        Inline::Image {
            destination,
            title,
            children,
        } => {
            out.push("<img src=\"");
            escape_attr(&encode_uri(destination), &mut out.0);
            out.push("\" alt=\"");
            escape_attr(&Inline::plain_text(children), &mut out.0);
            out.push("\"");
            if !title.is_empty() {
                out.push(" title=\"");
                escape_attr(title, &mut out.0);
                out.push("\"");
            }
            out.push(" />");
        }
        Inline::Html(h) => out.push(h),
        Inline::SoftBreak => out.push("\n"),
        Inline::HardBreak => out.push("<br />\n"),
        Inline::FootnoteReference {
            label,
            number,
            occurrence,
        } => {
            let label = encode_uri(label);
            out.push("<sup class=\"footnote-ref\"><a href=\"#fn-");
            escape_attr(&label, &mut out.0);
            out.push("\" id=\"fnref-");
            escape_attr(&label, &mut out.0);
            if *occurrence > 1 {
                out.push(&format!("-{occurrence}"));
            }
            out.push(&format!("\" data-footnote-ref>{number}</a></sup>"));
        }
        Inline::Math(tex) => {
            out.push("<span class=\"math inline\">\\(");
            escape_text(tex, &mut out.0);
            out.push("\\)</span>");
        }
    }
}

fn wrap(tag: &str, children: &[Inline], out: &mut Out) {
    out.push(&format!("<{tag}>"));
    inlines(children, out);
    out.push(&format!("</{tag}>"));
}
