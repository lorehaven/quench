//! Markdown laid out for a terminal: a [`quench_html`] syntax tree as styled lines for a given width.
//!
//! A second renderer beside `quench-html`'s HTML one, for `loupe` and anything else that shows Markdown in a
//! terminal. It is pure: a [`Document`] and [`Options`] in, [`Rendered`] lines out, with no I/O and no
//! terminal queries, so it is tested with fixed widths and snapshot text. Every line is at most
//! [`Options::width`] cells ([`display_width`] counts wide characters as two and combining marks as none).
//!
//! ```
//! use quench_html::markdown::{self, Options as Syntax};
//! use quench_markdown_term::{Options, render, to_plain};
//!
//! let document = markdown::parse("# Title\n\nSome *text*, and a list:\n\n- one\n- two\n", Syntax::ALL);
//! let rendered = render(&document, &Options { width: 20, ..Options::default() });
//! assert_eq!(
//!     to_plain(&rendered),
//!     "Title\n════════════════════\n\nSome text, and a\nlist:\n\n• one\n• two\n",
//! );
//! assert_eq!(rendered.headings[0].slug, "title");
//! ```
//!
//! What it lays out: paragraphs wrapped at spaces; headings by level; code blocks in a frame, wrapped with a
//! continuation mark rather than reflowed; quotes and lists indented, nested; tables sized to their content,
//! stacked as `header: value` records when even their narrowest form does not fit; footnotes at the end or
//! after their paragraph; formulas through a [`MathText`] hook, or as their TeX source. What Jekyll leaves in
//! a page is handled the way the site would show it: kramdown attribute lists are dropped (a `.note`,
//! `.warning`... class makes a callout), `{% include x %}` shows as `[include: x]`, `{{ site.baseurl }}` is
//! replaced (but not inside `{% raw %}`).
//!
//! Styles are semantic [`Role`]s; [`to_ansi`] colours them with a [`Theme`] and writes OSC 8 hyperlinks,
//! [`to_plain`] writes the text alone. The [`Rendered`] positions of headings, footnotes, links and formulas
//! are for a pager: following `#anchor` links, jumping to a footnote, putting a typeset formula over its text.

mod ansi;
mod inline;
mod layout;
mod model;
mod slug;
mod table;
#[cfg(feature = "tex")]
mod tex;
mod width;
mod wrap;

pub use ansi::{AnsiOptions, Colour, Theme, to_ansi, to_plain};
pub use inline::replace_baseurl;
pub use model::{
    CalloutKind, FootnoteSpot, FormulaSpot, HeadingSpot, Line, LinkKind, LinkTarget, Rendered,
    Role, Span, Style,
};
pub use slug::{Slugger, slug};
#[cfg(feature = "tex")]
pub use tex::TexMath;
pub use width::{char_width, display_width};

use quench_html::markdown::Document;

/// How a document is laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The width in terminal cells; every line fits in it. Widths under 4 are taken as 4.
    pub width: usize,
    /// Box drawing, bullets and superscripts; ASCII (`+-|`, `* - +`, footnote marks `[^1]`) when off.
    pub unicode: bool,
    pub footnotes: FootnotePlacement,
    pub math: MathMode,
    /// What `{{ site.baseurl }}` stands for in link destinations and text.
    pub baseurl: String,
    /// A dim `[n]` after each link whose destination is not its text, and the destinations listed at the
    /// end: for output without hyperlinks (pipes, `--plain`, terminals without OSC 8). Turn it off when the
    /// output is written with [`AnsiOptions::hyperlinks`]; it is part of the layout, so it is chosen here
    /// rather than by the writer, and lines stay within the width.
    pub numbered_links: bool,
}

impl Default for Options {
    /// 80 cells, Unicode, footnotes at the end, formulas as text, links numbered.
    fn default() -> Self {
        Self {
            width: 80,
            unicode: true,
            footnotes: FootnotePlacement::End,
            math: MathMode::Text,
            baseurl: String::new(),
            numbered_links: true,
        }
    }
}

/// Where footnote definitions go.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FootnotePlacement {
    /// At the end of the page under a rule, each with a back-mark `↩` to its reference.
    #[default]
    End,
    /// Each in a dim indented block right after the block holding its first reference.
    Inline,
}

/// How formulas are shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MathMode {
    /// As text from the [`MathText`] hook, or as source where it gives none.
    #[default]
    Text,
    /// Always as TeX source: inline as a dim `$tex$`, display in a dim frame.
    Source,
}

/// Turns TeX into terminal text. Where it returns `None` the formula's source is shown instead, whole: a
/// formula is never shown half-translated.
pub trait MathText {
    /// An inline formula as one line of text.
    fn inline(&self, tex: &str) -> Option<String>;
    /// A display formula as lines (laid out in two dimensions if it likes); they are centred as a block.
    fn display(&self, tex: &str) -> Option<Vec<String>>;
}

/// A [`MathText`] that translates nothing, so every formula shows as its source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceMath;

impl MathText for SourceMath {
    fn inline(&self, _tex: &str) -> Option<String> {
        None
    }

    fn display(&self, _tex: &str) -> Option<Vec<String>> {
        None
    }
}

/// Lays out `document` with `options`; formulas as their source.
#[must_use]
pub fn render(document: &Document, options: &Options) -> Rendered {
    render_with_math(document, options, &SourceMath)
}

/// Lays out `document` with `options`, formulas through `math`.
#[must_use]
pub fn render_with_math(document: &Document, options: &Options, math: &dyn MathText) -> Rendered {
    layout::Ctx::new(options, math, &document.footnotes).render(document)
}

/// Parses `markdown` with every extension (`quench_html::markdown::Options::ALL`) and lays it out.
///
/// `{{ site.baseurl }}` is replaced in the source before parsing, as Jekyll does: `[a]({{ site.baseurl }}/x)`
/// is only a link once the spaces are gone. Inside `{% raw %}` … `{% endraw %}` it is kept as written. Do the
/// same with [`replace_baseurl`] before calling [`render`] on a page parsed elsewhere.
#[must_use]
pub fn render_markdown(markdown: &str, options: &Options) -> Rendered {
    let source = replace_baseurl(markdown, &options.baseurl);
    render(
        &quench_html::markdown::parse(&source, quench_html::markdown::Options::ALL),
        options,
    )
}
