//! Shared styling for every Quench front end.
//!
//! One token set and one theme set serve server-rendered pages (`quench-web`) and the WASM UI
//! (`quench-ui`), which runs in a browser or in a mobile or desktop `WebView`. Platform differences
//! are small additive layers on top: [`mobile`] (`q-m-*`) and [`desktop`] (`q-d-*`).
//!
//! The crate has no third-party dependencies and builds for `wasm32-unknown-unknown`. The
//! functions that read `i18n/*.ftl` or write `dist/assets` use `std::fs`, which fails at run time
//! on that target, so call them from the server or the build tool only.

pub mod assets;
pub mod css;
pub mod desktop;
pub mod ftl;
pub mod i18n;
pub mod mobile;
pub mod styles;
pub mod theme;
pub mod tokens;

#[cfg(test)]
quench_test::main!();
