//! Inlines into styled pieces: emphasis, code, links, images, footnote references, formulas, and the Liquid
//! a Jekyll page leaves in its text.

use crate::layout::Ctx;
use crate::model::{LinkKind, LinkTarget, Role, Style};
use crate::width::display_width;
use crate::wrap::{MarkKind, Piece};
use crate::{FootnotePlacement, MathMode};
use quench_html::markdown::Inline;
use std::borrow::Cow;

const SUPERSCRIPTS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

/// A footnote number: in superscript digits, or as `[^n]` without Unicode (so it is not taken for a link's
/// ` [n]`).
pub fn superscript(number: usize, unicode: bool) -> String {
    if unicode {
        number
            .to_string()
            .bytes()
            .map(|b| SUPERSCRIPTS[usize::from(b - b'0')])
            .collect()
    } else {
        format!("[^{number}]")
    }
}

/// `text` without control characters; tabs and newlines become spaces. Keeps escape sequences in a page from
/// reaching the terminal.
pub fn clean(text: &str) -> String {
    text.chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect()
}

/// `text` without Liquid's `{% raw %}` / `{% endraw %}`, which Jekyll removes from code too.
pub fn strip_raw(text: &str) -> String {
    let mut out = text.to_owned();
    for tag in [
        "{% raw %}",
        "{% endraw %}",
        "{%raw%}",
        "{%endraw%}",
        "{%- raw -%}",
        "{%- endraw -%}",
    ] {
        out = out.replace(tag, "");
    }
    out
}

/// `text` with Jekyll's `{{ site.baseurl }}` replaced by `baseurl`, except inside `{% raw %}`.
///
/// The forms replaced are `{{ site.baseurl }}` with or without the spaces, or percent-encoded in a URL.
/// Inside `{% raw %}` … `{% endraw %}` Jekyll keeps it as written, and so does this; the raw tags are kept
/// too, for the renderer to remove. An unclosed `{% raw %}` keeps the rest of the text.
///
/// ```
/// use quench_markdown_term::replace_baseurl;
///
/// assert_eq!(replace_baseurl("[a]({{ site.baseurl }}/x)", "/book"), "[a](/book/x)");
/// assert_eq!(
///     replace_baseurl("{% raw %}{{ site.baseurl }}{% endraw %}", "/book"),
///     "{% raw %}{{ site.baseurl }}{% endraw %}",
/// );
/// ```
#[must_use]
pub fn replace_baseurl(text: &str, baseurl: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some((open, len)) = find_tag(rest, "raw") {
        out.push_str(&baseurl_forms(&rest[..open], baseurl));
        let inside = &rest[open + len..];
        // An unclosed raw block runs to the end, as the renderer reads it.
        let end = find_tag(inside, "endraw").map_or(inside.len(), |(at, len)| at + len);
        out.push_str(&rest[open..open + len + end]);
        rest = &inside[end..];
    }
    out.push_str(&baseurl_forms(rest, baseurl));
    out
}

/// `text` with every form of `{{ site.baseurl }}` replaced by `baseurl`.
fn baseurl_forms(text: &str, baseurl: &str) -> String {
    let mut out = text.to_owned();
    for form in [
        "{{ site.baseurl }}",
        "{{site.baseurl}}",
        "%7B%7B%20site.baseurl%20%7D%7D",
        "%7B%7Bsite.baseurl%7D%7D",
    ] {
        out = out.replace(form, baseurl);
    }
    out
}

enum Liquid {
    Text(String),
    Include(String),
}

/// Splits `text` at its Liquid tags: `{% include x %}` is kept as an include, the inside of `{% raw %}` as
/// text, other tags dropped; `{{ site.baseurl }}` is replaced by `baseurl` outside raw blocks only.
fn liquid(text: &str, baseurl: &str) -> Vec<Liquid> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("{%") {
        let Some(close) = rest[open..].find("%}") else {
            break;
        };
        literal.push_str(&baseurl_forms(&rest[..open], baseurl));
        let tag =
            rest[open + 2..open + close].trim_matches(|c: char| c == '-' || c.is_whitespace());
        rest = &rest[open + close + 2..];
        let mut words = tag.split_whitespace();
        match words.next() {
            Some("raw") => {
                let end = find_tag(rest, "endraw");
                let (inside, after) =
                    end.map_or((rest, ""), |(at, len)| (&rest[..at], &rest[at + len..]));
                literal.push_str(inside);
                rest = after;
            }
            Some("include" | "include_relative") => {
                parts.push(Liquid::Text(std::mem::take(&mut literal)));
                parts.push(Liquid::Include(words.next().unwrap_or("").to_owned()));
            }
            _ => {}
        }
    }
    literal.push_str(&baseurl_forms(rest, baseurl));
    parts.push(Liquid::Text(literal));
    parts
}

/// Where the tag `{% name %}` is in `text`, and how long it is.
fn find_tag(text: &str, name: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(open) = text[from..].find("{%") {
        let open = from + open;
        let close = text[open..].find("%}")?;
        let tag =
            text[open + 2..open + close].trim_matches(|c: char| c == '-' || c.is_whitespace());
        if tag == name {
            return Some((open, close + 2));
        }
        from = open + 2;
    }
    None
}

fn merged(inlines: &[Inline]) -> Vec<Cow<'_, Inline>> {
    let mut out: Vec<Cow<'_, Inline>> = Vec::new();
    for inline in inlines {
        if let (Inline::Text(text), Some(last)) = (inline, out.last_mut())
            && let Inline::Text(previous) = &**last
        {
            let joined = format!("{previous}{text}");
            *last = Cow::Owned(Inline::Text(joined));
            continue;
        }
        out.push(Cow::Borrowed(inline));
    }
    out
}

impl Ctx<'_> {
    /// `inlines` as pieces in `style`.
    pub fn pieces(&mut self, inlines: &[Inline], style: Style) -> Vec<Piece> {
        let mut out = Vec::new();
        self.inlines(inlines, style, &mut out);
        out
    }

    fn inlines(&mut self, inlines: &[Inline], style: Style, out: &mut Vec<Piece>) {
        for inline in merged(inlines) {
            self.inline(&inline, style, out);
        }
    }

    fn inline(&mut self, inline: &Inline, style: Style, out: &mut Vec<Piece>) {
        match inline {
            Inline::Text(text) => self.text(text, style, out),
            Inline::Code(code) => out.push(Piece::text(
                clean(&strip_raw(code)),
                Style {
                    code: true,
                    role: Role::Code,
                    ..style
                },
            )),
            Inline::Emph(children) => self.inlines(
                children,
                Style {
                    italic: true,
                    ..style
                },
                out,
            ),
            Inline::Strong(children) => self.inlines(
                children,
                Style {
                    bold: true,
                    ..style
                },
                out,
            ),
            Inline::Strikethrough(children) => {
                self.inlines(
                    children,
                    Style {
                        strikethrough: true,
                        ..style
                    },
                    out,
                );
            }
            Inline::Link {
                destination,
                title,
                children,
            } => self.link_inline(destination, title, children, style, out),
            Inline::Image {
                destination,
                title,
                children,
            } => self.image(destination, title, children, style, out),
            Inline::Html(html) => {
                out.push(Piece::text(
                    clean(html),
                    Style {
                        dim: true,
                        role: Role::Html,
                        ..style
                    },
                ));
            }
            Inline::SoftBreak => out.push(Piece::text(" ", style)),
            Inline::HardBreak => out.push(Piece::Break),
            Inline::FootnoteReference { number, .. } => {
                let index = self.footnote_link(*number, false);
                out.push(Piece::Mark(MarkKind::FootnoteRef(*number)));
                out.push(Piece::glued(
                    superscript(*number, self.options.unicode),
                    Style {
                        role: Role::Footnote,
                        link: Some(index),
                        ..style
                    },
                ));
            }
            Inline::Math(tex) => self.inline_math(tex, false, style, out),
        }
    }

    fn link_inline(
        &mut self,
        destination: &str,
        title: &str,
        children: &[Inline],
        style: Style,
        out: &mut Vec<Piece>,
    ) {
        let destination = replace_baseurl(destination, &self.options.baseurl);
        let shown = Inline::plain_text(children);
        let visible = shown == destination || format!("mailto:{shown}") == destination;
        let index = self.link(destination, title, LinkKind::Link, !visible);
        let linked = Style {
            underline: true,
            role: Role::Link,
            link: Some(index),
            ..style
        };
        self.inlines(children, linked, out);
        self.number(index, style, out);
    }

    fn image(
        &mut self,
        destination: &str,
        title: &str,
        children: &[Inline],
        style: Style,
        out: &mut Vec<Piece>,
    ) {
        let destination = replace_baseurl(destination, &self.options.baseurl);
        let index = self.link(destination, title, LinkKind::Image, true);
        let alt = clean(&Inline::plain_text(children));
        let text = if alt.trim().is_empty() {
            "[image]".to_owned()
        } else {
            format!("[image: {alt}]")
        };
        out.push(Piece::text(
            text,
            Style {
                role: Role::Image,
                link: Some(index),
                ..style
            },
        ));
        self.number(index, style, out);
    }

    fn text(&self, text: &str, style: Style, out: &mut Vec<Piece>) {
        for part in liquid(text, &self.options.baseurl) {
            match part {
                Liquid::Text(text) => out.push(Piece::text(clean(&text), style)),
                Liquid::Include(name) => out.push(Piece::text(
                    format!("[include: {}]", clean(&name)),
                    Style {
                        dim: true,
                        role: Role::Html,
                        ..style
                    },
                )),
            }
        }
    }

    fn inline_math(&mut self, tex: &str, display: bool, style: Style, out: &mut Vec<Piece>) {
        let text = match self.options.math {
            MathMode::Text => self
                .math
                .inline(tex)
                .map(|t| clean(&t))
                .filter(|t| !t.trim().is_empty()),
            MathMode::Source => None,
        };
        let (shown, style) = text.map_or_else(
            || {
                (
                    format!("${}$", clean(tex)),
                    Style {
                        dim: true,
                        role: Role::Math,
                        ..style
                    },
                )
            },
            |text| {
                (
                    text,
                    Style {
                        role: Role::Math,
                        ..style
                    },
                )
            },
        );
        let index = self.formula(tex, display, display_width(&shown), 1);
        out.push(Piece::Mark(MarkKind::Formula(index)));
        out.push(Piece::glued(shown, style));
    }

    /// Adds `[n]` after a link when links are numbered and the link has one.
    fn number(&self, index: usize, style: Style, out: &mut Vec<Piece>) {
        if let Some(number) = self.links[index].number {
            out.push(Piece::glued(
                format!(" [{number}]"),
                Style {
                    dim: true,
                    role: Role::Link,
                    link: Some(index),
                    ..style
                },
            ));
        }
    }

    /// Records a link; `numbered` when its destination is not already its text.
    fn link(&mut self, destination: String, title: &str, kind: LinkKind, numbered: bool) -> usize {
        let number = (self.options.numbered_links
            && numbered
            && !destination.is_empty()
            && !destination.starts_with('#'))
        .then(|| {
            let at = self
                .numbered
                .iter()
                .position(|d| *d == destination)
                .unwrap_or_else(|| {
                    self.numbered.push(destination.clone());
                    self.numbered.len() - 1
                });
            at + 1
        });
        self.links.push(LinkTarget {
            destination,
            title: title.to_owned(),
            kind,
            number,
        });
        self.links.len() - 1
    }

    /// The link of footnote `number`'s references (to its definition), or with `back` of its back-mark.
    pub fn footnote_link(&mut self, number: usize, back: bool) -> usize {
        let key = (number, back);
        if let Some(&index) = self.footnote_links.get(&key) {
            return index;
        }
        let label = self
            .footnotes
            .iter()
            .find(|f| f.number == number)
            .map_or_else(|| number.to_string(), |f| f.label.clone());
        let (destination, kind) = if back {
            (format!("#fnref-{label}"), LinkKind::FootnoteBack(number))
        } else {
            (format!("#fn-{label}"), LinkKind::Footnote(number))
        };
        self.links.push(LinkTarget {
            destination,
            title: String::new(),
            kind,
            number: None,
        });
        let index = self.links.len() - 1;
        self.footnote_links.insert(key, index);
        index
    }

    pub const fn inline_footnotes(&self) -> bool {
        matches!(self.options.footnotes, FootnotePlacement::Inline)
    }
}
