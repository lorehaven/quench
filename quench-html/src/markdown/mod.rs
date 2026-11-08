//! Markdown: `CommonMark` 0.31, and the GFM extensions and math as [`Options`].
//!
//! ```
//! use quench_html::markdown::{self, Block, Options, to_html, to_html_with};
//!
//! assert_eq!(to_html("Hello, *world*!\n"), "<p>Hello, <em>world</em>!</p>\n");
//! assert_eq!(to_html("- a\n- b\n"), "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n");
//! assert_eq!(to_html_with("~~gone~~\n", Options::GFM), "<p><del>gone</del></p>\n");
//!
//! let document = markdown::parse("| a | b |\n| - | - |\n| 1 | 2 |\n", Options::GFM);
//! assert!(matches!(document.blocks[0], Block::Table(_)));
//! ```
//!
//! [`to_html`] is plain `CommonMark` and passes its whole test suite. [`Options`] add GFM tables,
//! strikethrough, task lists and footnotes (written as cmark-gfm writes them, and tested with its examples)
//! and math (`$…$`, `$$…$$`, ```` ```math ````). [`parse`] gives the document as an [`ast`] for other
//! renderers. Raw HTML in the input is passed through unchanged; sanitise the output if the Markdown is not
//! trusted. Not read: smart punctuation, GFM's extended autolinks (`www.example.com` without `<>`), its tag
//! filter, definition lists.

pub mod ast;
mod blocks;
mod build;
mod inlines;
mod render;
mod scan;

pub use ast::{Alignment, Block, Document, Footnote, Inline, List, ListItem, Options, Table};

/// Parses `text` with the extensions of `options` into a [`Document`].
#[must_use]
pub fn parse(text: &str, options: Options) -> Document {
    let tree = blocks::parse(text, options);
    build::build(&tree, options)
}

/// Renders `CommonMark` `text` as HTML.
#[must_use]
pub fn to_html(text: &str) -> String {
    to_html_with(text, Options::COMMONMARK)
}

/// Renders `text` as HTML, with the extensions of `options`.
#[must_use]
pub fn to_html_with(text: &str, options: Options) -> String {
    render::render(&parse(text, options))
}

/// Renders a parsed `document` as HTML.
#[must_use]
pub fn render_html(document: &Document) -> String {
    render::render(document)
}
