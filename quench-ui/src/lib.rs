//! The platform-independent core of the Quench UI: reactivity, a view tree, and the renderer
//! interface a backend implements. It has no dependencies and no unsafe code, so it builds for the
//! browser (`wasm32-unknown-unknown`) and for native hosts alike.
//!
//! ```
//! use std::rc::Rc;
//! use quench_ui::prelude::*;
//!
//! let (renderer, root) = {
//!     let r = Rc::new(TestRenderer::new());
//!     let root = r.root();
//!     (r, root)
//! };
//! let count = create_signal(0);
//! let view = div()
//!     .child(span().text_fn({ let count = count.clone(); move || format!("count: {}", count.get()) }))
//!     .child(button().text("+").on_click({ let count = count.clone(); move || count.update(|c| *c += 1) }));
//! let mount = mount(&renderer, &root, view);
//!
//! assert_eq!(renderer.html(), "<div><span>count: 0</span><button>+</button></div>");
//! let button = renderer.find_by_tag("button")[0];
//! renderer.fire(button, &Event::Click);
//! assert_eq!(renderer.text(), "count: 1+");
//! mount.unmount();
//! ```
//!
//! Signals and views are `!Send`: a UI lives on one thread.

#![forbid(unsafe_code)]

pub mod anim;
pub mod event;
pub mod form;
pub mod gesture;
pub mod html;
pub mod i18n;
pub mod kit;
mod mount;
pub mod platform;
pub mod reactive;
pub mod renderer;
pub mod router;
pub mod test_renderer;
pub mod view;
pub mod virtual_list;

pub use event::{
    Event, EventKind, Handler, KeyEvent, PointerEvent, ScrollEvent, Size, prevent_default,
    take_prevent_request,
};
pub use mount::{Mount, mount};
pub use reactive::{
    Effect, Memo, Scope, Signal, batch, create_effect, create_memo, create_root, create_signal,
    on_cleanup, provide_context, untrack, use_context,
};
pub use renderer::{Prevent, Renderer};
pub use test_renderer::{Op, TestRenderer};
pub use view::{Element, IntoView, Key, View, element, fragment};

/// Everything a component usually needs.
pub mod prelude {
    pub use crate::event::{Event, EventKind, KeyEvent, PointerEvent};
    pub use crate::mount::{Mount, mount};
    pub use crate::reactive::{
        Effect, Memo, Scope, Signal, batch, create_effect, create_memo, create_root, create_signal,
        on_cleanup, provide_context, untrack, use_context,
    };
    pub use crate::renderer::Renderer;
    pub use crate::test_renderer::TestRenderer;
    pub use crate::view::{
        Element, IntoView, Key, View, a, button, div, element, footer, form, fragment, h1, h2, h3,
        header, i, img, input, label, li, main, nav, option, p, pre, section, select, span, strong,
        textarea, ul,
    };
}

/// Builds `view` into an in-memory tree and returns its HTML. Reactive parts render their current
/// value; listeners and effects are discarded.
#[must_use]
pub fn render_to_string(view: impl IntoView) -> String {
    use std::rc::Rc;
    let renderer = Rc::new(TestRenderer::new());
    let root = renderer.root();
    let (mounted, scope) = create_root(|| mount(&renderer, &root, view));
    let html = renderer.html();
    mounted.unmount();
    scope.dispose();
    html
}

#[cfg(test)]
quench_test::main!();
