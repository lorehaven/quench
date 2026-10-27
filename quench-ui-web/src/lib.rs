//! Runs a [`quench_ui`] app in the browser.
//!
//! The app compiles to WebAssembly and `quench.js` (see [`QUENCH_JS`]) gives it the DOM, timers
//! and `fetch`. The contract between the two is in `docs/quench-ui-web.md`.
//!
//! ```ignore
//! use quench_ui::prelude::*;
//!
//! fn app() -> View {
//!     let count = create_signal(0);
//!     let (shown, bump) = (count.clone(), count.clone());
//!     div()
//!         .child(span().text_fn(move || format!("count: {}", shown.get())))
//!         .child(button().text("+").on_click(move || bump.update(|c| *c += 1)))
//!         .into_view()
//! }
//! quench_ui_web::app!(app);
//! ```
//!
//! Off `wasm32` the JS side is replaced by [`mock`], so everything but the thin wasm imports runs
//! under `cargo test`.

mod dom;
mod events;
mod exec;
mod fetch;
mod ffi;
mod host;
mod state;

use quench_ui::{IntoView, Mount, mount};
use std::rc::Rc;

pub use dom::{DomRenderer, Node};
pub use events::{decode as decode_event, kind_code};
pub use exec::{Sleep, pending_tasks, sleep, spawn};
pub use fetch::{AbortHandle, Fetch, FetchError, Request, Response, pending_fetches};
pub use ffi::{
    q_alloc, q_dealloc, q_dispatch_event, q_fetch_done, q_set_state, q_tick, q_timer_fire,
};
#[cfg(not(target_arch = "wasm32"))]
pub use host::mock;
pub use state::initial_state;

/// The source of `quench.js`, for a server to serve at the URL the page loads it from.
pub const QUENCH_JS: &str = include_str!("../js/quench.js");

/// Builds `view` into the page's root and returns the mount, to unmount later.
pub fn mount_root(view: impl IntoView) -> Mount<DomRenderer> {
    let renderer = Rc::new(DomRenderer);
    mount(&renderer, &DomRenderer::root(), view)
}

/// Builds `view` over the server-rendered content already in the root, keeping its nodes.
///
/// The app builds the same view the server rendered. Each node it creates claims the next node of
/// the page instead, and attaches its listeners and effects to it. A node that does not match is
/// replaced by a fresh one, so the page ends up as a client-only render would have made it.
/// Returns the mount and how many nodes did not match (0 when the server and client agree).
pub fn hydrate_root(view: impl IntoView) -> (Mount<DomRenderer>, u32) {
    host::hydrate_begin();
    let mounted = mount_root(view);
    let mismatches = host::hydrate_end();
    (mounted, mismatches)
}

/// Sends panic messages to the console (and the development overlay) through the host.
pub fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| host::log(3, &info.to_string())));
}

/// What `q_main` runs: installs the panic hook, then hydrates the server-rendered page if there
/// is one and builds the app from scratch if not. The mount lives as long as the page.
pub fn run<V: IntoView>(app: impl FnOnce() -> V) {
    install_panic_hook();
    if host::root_has_content() {
        hydrate_root(app()).0.leak();
    } else {
        mount_root(app()).leak();
    }
}

/// Forgets listeners, tasks, timers and fetches. For tests; the host's own state has its own reset.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn reset_runtime() {
    dom::reset();
    state::reset();
    exec::reset();
    fetch::reset();
}

/// Defines the `q_main` export that starts an app: `quench_ui_web::app!(my_app);` where
/// `my_app` is a `fn() -> impl IntoView`.
#[macro_export]
macro_rules! app {
    ($app:path) => {
        #[cfg_attr(target_arch = "wasm32", unsafe(no_mangle))]
        pub extern "C" fn q_main() {
            $crate::run($app);
        }
    };
}

#[cfg(test)]
quench_test::main!();
