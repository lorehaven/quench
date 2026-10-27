//! The router's [`History`] over the browser's session history.

use std::cell::RefCell;
use std::rc::Rc;

use quench_ui::router::{History, Location};

use crate::host;

type Listener = Rc<dyn Fn(Location)>;

struct State {
    current: Location,
    listeners: Vec<Listener>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { current: Location::parse("/"), listeners: Vec::new() });
}

/// Sets where the page was loaded at, before `q_main`.
pub fn set_initial(url: &str) {
    STATE.with(|s| s.borrow_mut().current = Location::parse(url));
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    let old = STATE.with(|s| {
        std::mem::replace(
            &mut *s.borrow_mut(),
            State {
                current: Location::parse("/"),
                listeners: Vec::new(),
            },
        )
    });
    drop(old);
}

/// The browser moved to `url` on its own (the back and forward buttons).
pub fn popstate(url: &str) {
    let location = Location::parse(url);
    let listeners = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.current = location.clone();
        s.listeners.clone()
    });
    for listener in listeners {
        listener(location.clone());
    }
}

/// The browser's history: the URL bar, the back and forward buttons, and `pushState`. Scroll
/// positions are saved and restored by `quench.js`.
#[derive(Clone, Copy, Debug, Default)]
pub struct BrowserHistory;

impl BrowserHistory {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl History for BrowserHistory {
    fn location(&self) -> Location {
        STATE.with(|s| s.borrow().current.clone())
    }

    fn push(&self, location: &Location) {
        STATE.with(|s| s.borrow_mut().current = location.clone());
        host::history(0, &location.url());
    }

    fn replace(&self, location: &Location) {
        STATE.with(|s| s.borrow_mut().current = location.clone());
        host::history(1, &location.url());
    }

    fn back(&self) -> bool {
        host::history(2, "");
        true
    }

    fn forward(&self) -> bool {
        host::history(3, "");
        true
    }

    fn subscribe(&self, listener: Rc<dyn Fn(Location)>) {
        STATE.with(|s| s.borrow_mut().listeners.push(listener));
    }
}
