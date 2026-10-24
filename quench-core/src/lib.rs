//! Dependency-free foundations shared by every quench crate.
//!
//! Replaces `chrono`, `uuid`, `base32`, `base64`, `anyhow`, `envmnt`, `dashmap`,
//! `semver`, `bytes`, `inventory`, `tempfile`, `dotenvy`, `futures-util`, `walkdir`, `which` and `shlex`. Everything except [`random`] and the
//! filesystem helpers is pure and builds for `wasm32-unknown-unknown`.

pub mod base32;
pub mod base64;
pub mod bytes;
pub mod date;
pub mod dotenv;
pub mod env;
pub mod error;
pub mod fs;
pub mod future;
pub mod hex;
pub mod ignore;
pub mod map;
pub mod process;
pub mod random;
pub mod registry;
pub mod shell;
pub mod stream;
pub mod time;
pub mod tmp;
pub mod uuid;
pub mod version;

pub use error::{Context, Result};
pub use quench_core_macros::{Error, async_trait};
