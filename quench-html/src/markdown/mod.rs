//! `CommonMark` to HTML.
//!
//! ```
//! use quench_html::markdown::to_html;
//!
//! assert_eq!(to_html("Hello, *world*!\n"), "<p>Hello, <em>world</em>!</p>\n");
//! assert_eq!(to_html("- a\n- b\n"), "<ul>\n<li>a</li>\n<li>b</li>\n</ul>\n");
//! ```
//!
//! This implements `CommonMark` 0.31 only: no tables, footnotes, strikethrough, task lists or smart
//! punctuation. Raw HTML in the input is passed through unchanged; sanitise the output if the Markdown is
//! not trusted.

mod blocks;
mod inlines;
mod render;
mod scan;

/// Renders `CommonMark` `text` as HTML.
#[must_use]
pub fn to_html(text: &str) -> String {
    let document = blocks::parse(text);
    render::render(&document)
}
