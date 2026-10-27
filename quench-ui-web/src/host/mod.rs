//! The calls into the JavaScript side. On `wasm32` they are the imports of `quench.js`; on every
//! other target they are an in-memory stand-in that tests drive (see [`mock`]).

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(not(target_arch = "wasm32"))]
pub mod mock;
#[cfg(not(target_arch = "wasm32"))]
pub use mock::imports::*;

/// The handle of the root container.
pub const ROOT: u32 = 1;
