//! The browser's clock, timers and motion preference for `quench_ui::platform`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use quench_ui::platform::Platform;

use crate::exec::{sleep, spawn};
use crate::host;

/// What [`quench_ui::platform::install`] gets in the browser; [`crate::run`] installs it.
#[derive(Debug, Default)]
pub struct WebPlatform {
    next: Cell<u32>,
    live: Rc<RefCell<HashMap<u32, Rc<Cell<bool>>>>>,
}

impl WebPlatform {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Platform for WebPlatform {
    fn now(&self) -> f64 {
        host::now()
    }

    fn after(&self, ms: u32, f: Box<dyn FnOnce()>) -> u32 {
        let id = self.next.get() + 1;
        self.next.set(id);
        let cancelled = Rc::new(Cell::new(false));
        self.live.borrow_mut().insert(id, cancelled.clone());
        let live = self.live.clone();
        spawn(async move {
            sleep(ms).await;
            live.borrow_mut().remove(&id);
            if !cancelled.get() {
                f();
            }
        });
        id
    }

    fn cancel(&self, id: u32) {
        if let Some(flag) = self.live.borrow_mut().remove(&id) {
            flag.set(true);
        }
    }

    fn prefers_reduced_motion(&self) -> bool {
        host::prefers_reduced_motion()
    }
}
