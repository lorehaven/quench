//! The state the server embedded in the page.

use std::cell::RefCell;

thread_local! {
    static STATE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn set(json: String) {
    STATE.with(|s| *s.borrow_mut() = Some(json));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    STATE.with(|s| *s.borrow_mut() = None);
}

/// The JSON text of the page's `<script type="application/json" id="__q_state">`, if the server
/// sent one. Parse it with the same types the server serialised.
#[must_use]
pub fn initial_state() -> Option<String> {
    STATE.with(|s| s.borrow().clone())
}
