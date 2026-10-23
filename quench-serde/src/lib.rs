//! JSON, TOML and form encodings plus a dynamic [`Value`], without `serde`.
//!
//! Types convert through [`Value`]: parse text into a `Value`, then read it
//! into a type (or the other way round). That one detour keeps every format
//! small and lets them share the conversion code.

// The derive macros name this crate by absolute path; this lets them do so
// from inside it too.
extern crate self as quench_serde;

pub mod error;
pub mod json;
mod macros;
mod map;
#[doc(hidden)]
pub mod private;
#[cfg(feature = "toml")]
pub mod toml;
mod traits;
pub mod urlencoded;
mod value;
#[cfg(feature = "yaml")]
pub mod yaml;

pub use error::{Error, Result};
pub use map::Map;
pub use traits::{
    Deserialize, DeserializeOwned, FromKey, Serialize, ToKey, from_value, invalid_type, to_value,
};
pub use value::{Value, ValueIndex};

// The derive macros share their names with the traits: macros and types live
// in different namespaces, so both can be imported as `Serialize`.
pub use quench_serde_macros::{Deserialize, Serialize};

#[doc(hidden)]
pub use private as __private;
