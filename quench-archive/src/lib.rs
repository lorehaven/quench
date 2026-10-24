//! Archives and compression with no dependencies.
//!
//! - [`tar`]: ustar, GNU long names and pax headers, read and written.
//! - [`zip`]: stored and deflated entries, read and written.
//! - [`deflate`], [`gzip`]: RFC 1951/1952, decoder and encoder.
//! - [`zstd`]: RFC 8878, decoder and encoder.
//! - [`checksum`]: CRC-32, Adler-32, xxHash64.
//!
//! Everything works on byte slices and `Vec<u8>`, and every decoder takes a limit on the size of its output
//! so a small hostile input cannot expand without bound.

// Binary formats pack small fields out of wider integers; each narrowing here is of a value the format (or a
// mask or a length check just above it) bounds.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

pub mod checksum;
pub mod deflate;
pub mod error;
pub mod gzip;
mod huffman;
pub mod tar;
pub mod zip;
pub mod zstd;

pub use error::{Error, Result};

#[cfg(test)]
quench_test::main!();
