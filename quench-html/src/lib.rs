//! HTML without dependencies.
//!
//! - [`mod@markdown`] (feature `markdown`): `CommonMark` to HTML, passing the whole `CommonMark` 0.31 test suite.
//! - [`dom`] and [`selector`] (feature `parse`): a tolerant HTML parser that builds a document tree, and a
//!   CSS selector engine to query it.
//!
//! ```
//! # #[cfg(all(feature = "parse", feature = "markdown"))] {
//! use quench_html::{Document, Selector, markdown};
//!
//! let html = markdown::to_html("# Title\n\nSome *emphasis*.\n");
//! let document = Document::parse(&html);
//! let em = Selector::parse("p em").unwrap();
//! assert_eq!(document.select(&em).next().unwrap().text(), "emphasis");
//! # }
//! ```

// Character offsets and counts in text scanners are bounded by the input they came from.
#![allow(
    clippy::assigning_clones,
    clippy::format_push_string,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::option_if_let_else,
    clippy::or_fun_call
)]

pub mod entities;

#[cfg(feature = "markdown")]
pub mod markdown;

#[cfg(feature = "parse")]
pub mod dom;
#[cfg(feature = "parse")]
pub mod selector;

#[cfg(feature = "parse")]
pub use dom::{Document, Node, NodeKind};
#[cfg(feature = "parse")]
pub use selector::{Selector, SelectorError};

/// Decodes the character references in `text` (`&amp;`, `&#65;`, `&#x41;`, named references; the legacy
/// ones also without the `;`). Unknown references are left as they are.
#[must_use]
pub fn decode_entities(text: &str) -> String {
    entities::decode(text, false)
}
