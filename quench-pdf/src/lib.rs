//! A PDF reader that extracts text page by page.
//!
//! ```no_run
//! let bytes = std::fs::read("report.pdf").unwrap();
//! for (number, text) in quench_pdf::extract_pages(&bytes).unwrap().iter().enumerate() {
//!     println!("--- page {} ---\n{text}", number + 1);
//! }
//! ```
//!
//! The reader follows cross-reference tables and streams (rebuilding them by scanning when they are
//! damaged), object streams, the page tree, and content streams including form `XObjects`. Fonts are decoded
//! through `ToUnicode` maps, the base encodings and `Differences`, and composite (CID) fonts; spacing and
//! line breaks come from glyph positions. Encrypted files are reported as [`Error::Unsupported`]; text
//! drawn as outlines or images is not text and is not extracted.

// PDF stores small integers in wider ones and positions as floats; each narrowing is of a value the format
// bounds or a glyph coordinate.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::option_if_let_else,
    clippy::or_fun_call,
    clippy::single_match_else,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::unnecessary_wraps
)]

mod cmap;
mod doc;
mod error;
mod filter;
mod font;
mod glyphs;
mod object;
mod text;

pub use doc::Document;
pub use error::{Error, Result};

use std::collections::HashMap;

/// The text of every page, in page order.
///
/// # Errors
///
/// [`Error::Malformed`] when the data is not a readable PDF, [`Error::Unsupported`] for encrypted ones.
pub fn extract_pages(data: &[u8]) -> Result<Vec<String>> {
    let doc = Document::load(data)?;
    let mut fonts = HashMap::new();
    Ok(doc
        .pages()
        .iter()
        .map(|page| text::page_text(&doc, page, &mut fonts))
        .collect())
}

/// All the text, pages separated by a blank line.
///
/// # Errors
///
/// As [`extract_pages`].
pub fn extract_text(data: &[u8]) -> Result<String> {
    Ok(extract_pages(data)?.join("\n\n"))
}
