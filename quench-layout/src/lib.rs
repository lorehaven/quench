//! Box layout with no dependencies: block and inline flow, flexbox, positioning and margin collapsing over the
//! computed styles of `quench-css` and the text of `quench-text`.
//!
//! [`layout`] takes a tree of [`Node`]s and the size of the viewport and returns a tree of [`LayoutBox`]es.

// Pixel arithmetic in `f32`, and the short names of the box model.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::suboptimal_flops,
    clippy::float_cmp,
    clippy::match_same_arms,
    clippy::unused_self,
    clippy::use_self,
    clippy::option_if_let_else
)]

mod engine;
mod flex;
mod fonts;
mod inline;
mod intrinsic;
mod node;
mod position;

pub use fonts::{Fonts, Metrics};
pub use inline::LINE_BREAK;
pub use node::{Content, Fragment, InlineFragment, LayoutBox, Node, TextFragment};
pub use position::layout;
