//! Serves [`quench_ui`] apps from [`quench_http`]: the page is rendered on the server, the browser
//! hydrates it with the same code, and everything is served with a strict
//! Content-Security-Policy.
//!
//! ```ignore
//! let ui = Ui::new(std::fs::read("app.wasm")?);
//! let app = UiApp::new(ui).page("/", |_req| {
//!     Page::new("Hello", my_app::view(my_app::State::load())).state(my_app::State::load().json())
//! });
//! serve_http(Arc::new(app), container, addr).await?;
//! ```
//!
//! What a page contains, and what the browser does with it, is described in
//! `docs/quench-fullstack.md`.

mod app;
mod assets;
mod document;

pub use app::{Ui, UiApp};
pub use assets::{Asset, Assets, PREFIX, content_hash};
pub use document::{
    Page, ROOT_ID, STATE_ID, content_security_policy, escape_json, new_nonce, render_document,
};

#[cfg(test)]
quench_test::main!();
