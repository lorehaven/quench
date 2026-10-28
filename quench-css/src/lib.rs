//! A CSS engine with no dependencies.
//!
//! [`parse`] reads a stylesheet (nesting resolved, `@media` and `@supports` evaluated later or now, keyframes kept),
//! selectors match any tree that implements [`Element`], and the cascade computes a style for an element.

// Pixel and channel arithmetic in `f32`, and the short names CSS uses for its components (r, g, b, w, h).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::struct_excessive_bools,
    clippy::too_many_lines,
    clippy::suboptimal_flops,
    clippy::use_self
)]

mod color;
mod length;
mod media;
mod properties;
mod selector;
mod sheet;
mod style;
mod token;
mod value;

pub use color::{Color, ColorValue, parse_color};
pub use length::{Calc, Length, Units, parse_length};
pub use media::{ColorScheme, MediaEnv, MediaQuery, Pointer, all_match, parse_media_queries};
pub use properties::{IGNORED, SUPPORTED, is_known};
pub use selector::{
    Combinator, Complex, Compound, Element, Simple, Specificity, State, parse_selector_list,
    resolve_nesting,
};
pub use sheet::{Declaration, Keyframes, Rule, Stylesheet, parse, parse_declarations};
pub use style::{
    Align, BorderStyle, Cascade, ComputedStyle, Display, FlexDirection, Justify, LineHeight,
    ListStyle, Overflow, Position, Shadow, TextAlign, TextTransform, Transform, accepts,
};
pub use token::{Token, tokenize};
pub use value::{Value, parse_values, split_commas};
