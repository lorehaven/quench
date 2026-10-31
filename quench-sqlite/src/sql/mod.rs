//! SQL text: the lexer, the syntax tree and the parser.
//!
//! The subset is SQLite's dialect for the statements an application issues (tables, indexes, inserts with
//! upserts, updates, deletes, selects with joins, grouping and ordering, transactions, pragmas). Syntax outside
//! it is refused with `Error::Unsupported` naming the feature, never parsed wrongly.

pub mod ast;
mod lexer;
mod parser;

pub use parser::{parse, parse_one, parse_with_text};
