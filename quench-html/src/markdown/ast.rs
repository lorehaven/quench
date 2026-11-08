//! The parsed document, for renderers other than HTML: blocks with their inlines read, footnotes numbered.
//!
//! [`parse`](super::parse) gives a [`Document`]; [`to_html`](super::to_html_with) is one renderer of it, a
//! terminal renderer another. The tree is plain data: everything a renderer needs is in it, nothing of the
//! parser's state is.

/// Which extensions to `CommonMark` are read. [`Options::default`] is plain `CommonMark`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Options {
    /// GFM tables: a header row, a delimiter row (`| --- | :-: |`), then rows.
    pub tables: bool,
    /// GFM strikethrough: `~text~` and `~~text~~`.
    pub strikethrough: bool,
    /// GFM task list items: `- [ ] to do`, `- [x] done`.
    pub task_lists: bool,
    /// Footnotes (GFM and kramdown syntax): `text[^note]` and `[^note]: the note`.
    pub footnotes: bool,
    /// Math: `$inline$`, `$$inline$$` inside a paragraph, and display math as a paragraph that is only
    /// `$$...$$`, a block between `$$` lines, or a fenced `math` code block.
    pub math: bool,
}

impl Options {
    /// Plain `CommonMark` 0.31.
    pub const COMMONMARK: Self = Self {
        tables: false,
        strikethrough: false,
        task_lists: false,
        footnotes: false,
        math: false,
    };

    /// What GitHub reads: tables, strikethrough, task lists and footnotes.
    pub const GFM: Self = Self {
        tables: true,
        strikethrough: true,
        task_lists: true,
        footnotes: true,
        math: false,
    };

    /// Every extension: GFM and math.
    pub const ALL: Self = Self {
        tables: true,
        strikethrough: true,
        task_lists: true,
        footnotes: true,
        math: true,
    };
}

/// A parsed document: its blocks, and the footnotes they reference in the order of their first reference.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    /// Only the footnotes that are referenced; numbered from 1 in the order of the first reference.
    pub footnotes: Vec<Footnote>,
}

/// A block of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Paragraph(Vec<Inline>),
    Heading {
        /// 1 to 6.
        level: u8,
        content: Vec<Inline>,
    },
    ThematicBreak,
    CodeBlock {
        /// The info string of a fenced block (`rust`, `toml title="x"`); empty for an indented one.
        info: String,
        /// The code, ending with a newline unless empty.
        literal: String,
        fenced: bool,
    },
    /// Raw HTML, passed on as it is.
    Html(String),
    BlockQuote(Vec<Self>),
    List(List),
    Table(Table),
    /// A display formula: its TeX, without the `$$` or the fence.
    Math(String),
}

/// A bullet or ordered list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct List {
    pub ordered: bool,
    /// The first number of an ordered list.
    pub start: usize,
    /// Tight: no blank lines between items, so paragraphs in items are not set apart.
    pub tight: bool,
    /// `-`, `+` or `*` for a bullet list; `.` or `)` for an ordered one.
    pub marker: char,
    pub items: Vec<ListItem>,
}

/// An item of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListItem {
    /// A task list item: `Some(true)` checked, `Some(false)` not; `None` for an ordinary item.
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

/// How a table column is aligned.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Alignment {
    #[default]
    None,
    Left,
    Center,
    Right,
}

/// A GFM table. Every row has as many cells as the header: short rows are filled with empty cells, extra
/// cells are dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub alignments: Vec<Alignment>,
    pub header: Vec<Vec<Inline>>,
    pub rows: Vec<Vec<Vec<Inline>>>,
}

/// A footnote that is referenced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Footnote {
    /// The label as written in the definition (`note` of `[^note]:`).
    pub label: String,
    /// From 1, in the order of the first reference.
    pub number: usize,
    /// How many times it is referenced (at least 1).
    pub references: usize,
    pub blocks: Vec<Block>,
}

/// Inline content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Self>),
    Strong(Vec<Self>),
    Strikethrough(Vec<Self>),
    Link {
        destination: String,
        title: String,
        children: Vec<Self>,
    },
    Image {
        destination: String,
        title: String,
        /// The alt text, as inlines.
        children: Vec<Self>,
    },
    /// Raw inline HTML.
    Html(String),
    SoftBreak,
    HardBreak,
    /// `[^label]` of a footnote that is defined.
    FootnoteReference {
        /// The definition's label.
        label: String,
        /// The footnote's number.
        number: usize,
        /// Which reference to it this is: 1 for the first, 2 for the second...
        occurrence: usize,
    },
    /// An inline formula, its TeX: `$x$`, or `$$x$$` inside a paragraph. A paragraph that is only `$$x$$`
    /// becomes a display [`Block::Math`] instead.
    Math(String),
}

impl Inline {
    /// The text of `inlines` without markup: what a link or an image's alt text says.
    #[must_use]
    pub fn plain_text(inlines: &[Self]) -> String {
        let mut out = String::new();
        plain_text_into(inlines, &mut out);
        out
    }
}

fn plain_text_into(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Math(tex) => out.push_str(tex),
            Inline::Emph(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                plain_text_into(c, out);
            }
            Inline::Link { children, .. } | Inline::Image { children, .. } => {
                plain_text_into(children, out);
            }
            Inline::SoftBreak | Inline::HardBreak => out.push('\n'),
            Inline::FootnoteReference { number, .. } => out.push_str(&number.to_string()),
            Inline::Html(_) => {}
        }
    }
}
