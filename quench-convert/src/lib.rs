//! Readers and writers that turn other formats into [`quench_serde::Value`]s.
//!
//! Each format is a module behind a feature (all on by default):
//!
//! - [`csv`]: delimited text, as arrays or as objects keyed by the header row.
//! - [`xml`]: XML text as an element tree of plain values, and back.
//! - [`axml`]: Android binary XML (the `AndroidManifest.xml` of an APK), as the same tree.
//! - [`gguf`]: the header and metadata of GGUF model files.
//!
//! Because everything ends up as a `Value`, the result can be queried directly, serialised with the
//! other `quench-serde` formats, or turned into a struct with `quench_serde::from_value`.

// Binary formats read small fields out of wider integers; each narrowing is of a value the format bounds.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::option_if_let_else,
    clippy::or_fun_call
)]

mod error;

#[cfg(feature = "axml")]
pub mod axml;
#[cfg(feature = "csv")]
pub mod csv;
#[cfg(feature = "gguf")]
pub mod gguf;
#[cfg(feature = "xml")]
pub mod xml;

pub use error::{Error, Result};
pub use quench_serde::Value;
