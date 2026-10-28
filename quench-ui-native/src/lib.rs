//! A [`quench_ui::Renderer`] that draws itself.
//!
//! The renderer keeps a DOM in memory, like [`quench_ui::TestRenderer`]. [`NativeRenderer::render`] runs the whole
//! pipeline on it: `quench-css` styles every element, `quench-layout` places the boxes, `quench-paint` draws them
//! into a canvas. There is no `WebView` and no browser: a window only has to show the pixels.

// Pixel arithmetic in `f32`.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::redundant_pub_crate,
    clippy::items_after_statements,
    clippy::redundant_guards,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names
)]

mod access;
mod dom;
mod input;
mod pipeline;

pub use access::{AccessAction, AccessNode, Role};
pub use dom::{NativeRenderer, NodeId, NodeKind};
pub use input::ImeState;
pub use pipeline::UA_STYLESHEET;
pub use quench_css::{ColorScheme, MediaEnv, Pointer};
