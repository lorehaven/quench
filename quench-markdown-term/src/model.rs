//! What rendering gives: styled lines, and where the things a pager jumps to ended up.
//!
//! Styles are semantic ([`Role`]s, not colours), so one rendering serves a dark terminal, a light one and a
//! pipe; a [`Theme`](crate::Theme) picks the colours when the lines are written.

use crate::width::display_width;

/// A document laid out for a width.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    /// The lines, each at most the width in cells; blank lines have no spans.
    pub lines: Vec<Line>,
    /// The links, images and footnote marks; [`Style::link`] indexes this.
    pub links: Vec<LinkTarget>,
    /// The headings, in order, with the line each starts on.
    pub headings: Vec<HeadingSpot>,
    /// The footnotes, by number, with where their definitions and references are.
    pub footnotes: Vec<FootnoteSpot>,
    /// The formulas, in order, with the area each takes.
    pub formulas: Vec<FormulaSpot>,
}

impl Rendered {
    /// The line where the heading with anchor `slug` starts, for following a `#slug` link.
    #[must_use]
    pub fn heading_line(&self, slug: &str) -> Option<usize> {
        self.headings
            .iter()
            .find(|h| h.slug == slug)
            .map(|h| h.line)
    }
}

/// One line of output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    /// The cells the line takes.
    #[must_use]
    pub fn width(&self) -> usize {
        display_width(&self.text())
    }

    /// The line's text, without styles.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Whether the line is blank.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// Appends `text` in `style`, joining it to the last span when the style is the same.
    pub fn push(&mut self, text: &str, style: Style) {
        if text.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last) if last.style == style => last.text.push_str(text),
            _ => self.spans.push(Span {
                text: text.to_owned(),
                style,
            }),
        }
    }

    /// Appends the spans of `other`.
    pub fn extend(&mut self, other: &Self) {
        for span in &other.spans {
            self.push(&span.text, span.style);
        }
    }
}

/// Text in one style.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

/// How a span looks: attributes, a semantic role a theme colours, and the link it belongs to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
    /// Code (a code span or block): monospace in a terminal already, so only a theme colour marks it.
    pub code: bool,
    pub role: Role,
    /// The index in [`Rendered::links`] of the link this span is part of.
    pub link: Option<usize>,
}

/// What a span is, for a theme to colour.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Role {
    /// Body text.
    #[default]
    None,
    /// A heading of this level (1 to 6), and its rule.
    Heading(u8),
    Link,
    Image,
    Code,
    /// A block quote's bar.
    Quote,
    /// Thematic breaks, frames and table borders.
    Rule,
    /// List bullets, numbers and task boxes; a code block's language.
    Marker,
    Math,
    /// Raw HTML, and Liquid tags shown.
    Html,
    /// Footnote references, back-marks and numbers.
    Footnote,
    /// A callout's bar and label.
    Callout(CalloutKind),
}

/// The callouts of just-the-docs: a block after `{: .note }`, `{: .warning }`...
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalloutKind {
    Note,
    Tip,
    Important,
    Warning,
    Highlight,
    New,
}

impl CalloutKind {
    /// The kind a kramdown class names (`note`, `warning`, ...), if it is a callout.
    #[must_use]
    pub fn from_class(class: &str) -> Option<Self> {
        Some(match class {
            "note" => Self::Note,
            "tip" => Self::Tip,
            "important" => Self::Important,
            "warning" => Self::Warning,
            "highlight" => Self::Highlight,
            "new" => Self::New,
            _ => return None,
        })
    }

    /// The label shown above the callout.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Highlight => "Highlight",
            Self::New => "New",
        }
    }
}

/// Where a link goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    /// The destination, with `{{ site.baseurl }}` replaced; `#fn-label` / `#fnref-label` for footnotes.
    pub destination: String,
    pub title: String,
    pub kind: LinkKind,
    /// The `[n]` shown after the link's text and listed at the end, when links are numbered.
    pub number: Option<usize>,
}

/// What a [`LinkTarget`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Link,
    Image,
    /// A reference to the footnote with this number: goes to its definition.
    Footnote(usize),
    /// A definition's back-mark: goes to the first reference of the footnote with this number.
    FootnoteBack(usize),
}

impl LinkTarget {
    /// Whether a terminal can open it as an OSC 8 hyperlink: a link or image with a URL scheme (`https:`,
    /// `mailto:`). Relative links and anchors are for a pager to follow.
    #[must_use]
    pub fn is_url(&self) -> bool {
        matches!(self.kind, LinkKind::Link | LinkKind::Image) && has_scheme(&self.destination)
    }
}

pub fn has_scheme(destination: &str) -> bool {
    let Some((scheme, _)) = destination.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// A heading and the line it starts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingSpot {
    pub level: u8,
    /// Its text, without markup.
    pub text: String,
    /// Its anchor: the `{#id}` it gives, or the slug of its text.
    pub slug: String,
    pub line: usize,
}

/// A footnote: where its definition is and where it is referenced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteSpot {
    pub number: usize,
    pub label: String,
    /// The line its definition starts on.
    pub definition_line: usize,
    /// The lines of its references, in order.
    pub reference_lines: Vec<usize>,
}

/// A formula and the area it was laid out in, for a pager to put a typeset image over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaSpot {
    /// The first line it takes.
    pub line: usize,
    /// The cell it starts at on that line.
    pub column: usize,
    /// The cells it takes across.
    pub width: usize,
    /// The lines it takes.
    pub height: usize,
    pub tex: String,
    /// A display formula (a block of its own), not inline.
    pub display: bool,
}
