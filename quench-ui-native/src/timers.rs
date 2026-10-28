//! Time for the views: the clock and timers `quench_ui::platform` asks for (toasts that go away, delays, debounces),
//! and a way for other threads to hand results to the views.
//!
//! The host calls [`NativeRenderer::tick`] whenever it wakes (after [`NativeRenderer::next_wake`] at the latest);
//! the timers that are due run there, on the thread of the views, and so do the handlers of the channels.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use quench_ui::platform::Platform;

use crate::dom::NativeRenderer;

type Timer = (u32, Instant, Box<dyn FnOnce()>);

/// The platform of a native window: a monotonic clock and one-shot timers that run when the host ticks.
pub(crate) struct NativePlatform {
    start: Instant,
    next: Cell<u32>,
    reduced: Cell<bool>,
    timers: RefCell<Vec<Timer>>,
}

impl NativePlatform {
    pub(crate) fn new() -> Self {
        Self {
            start: Instant::now(),
            next: Cell::new(1),
            reduced: Cell::new(false),
            timers: RefCell::new(Vec::new()),
        }
    }

    pub(crate) fn set_reduced_motion(&self, on: bool) {
        self.reduced.set(on);
    }

    /// Runs what is due; returns whether anything was.
    pub(crate) fn run_due(&self) -> bool {
        let mut ran = false;
        loop {
            let now = Instant::now();
            let due = {
                let mut timers = self.timers.borrow_mut();
                let first = timers
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.1 <= now)
                    .min_by_key(|(_, t)| t.1)
                    .map(|(i, _)| i);
                first.map(|i| timers.remove(i))
            };
            let Some((_, _, f)) = due else { return ran };
            f();
            ran = true;
        }
    }

    pub(crate) fn until_next(&self) -> Option<Duration> {
        let now = Instant::now();
        self.timers
            .borrow()
            .iter()
            .map(|t| t.1.saturating_duration_since(now))
            .min()
    }
}

impl Platform for NativePlatform {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn after(&self, ms: u32, f: Box<dyn FnOnce()>) -> u32 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.timers.borrow_mut().push((
            id,
            Instant::now() + Duration::from_millis(u64::from(ms)),
            f,
        ));
        id
    }

    fn cancel(&self, id: u32) {
        self.timers.borrow_mut().retain(|t| t.0 != id);
    }

    fn prefers_reduced_motion(&self) -> bool {
        self.reduced.get()
    }
}

/// How often frames come while a transition runs.
const FRAME: Duration = Duration::from_millis(16);

/// How often a host with channels should wake to look at them.
const POLL: Duration = Duration::from_millis(25);

impl NativeRenderer {
    /// Runs the timers that are due and delivers what other threads sent through [`channel`](Self::channel).
    /// Returns whether anything ran or a transition moved (the page may have changed: [`take_dirty`](Self::take_dirty) says).
    pub fn tick(&self) -> bool {
        let mut ran = self.platform.run_due();
        // The handlers run with no borrow held: they change the views, and may open channels.
        let pollers: Vec<Rc<dyn Fn() -> bool>> = self.pollers.borrow().clone();
        for poll in pollers {
            ran |= poll();
        }
        // A transition under way needs a frame after each step.
        let animating = self.dom.borrow().anim.borrow().active();
        ran |= animating;
        if ran {
            let mut dom = self.dom.borrow_mut();
            dom.ui.dirty = true;
            dom.ui.stale = true;
        }
        ran
    }

    /// How long until the next timer is due, or until channels should be looked at again; `None` when nothing
    /// will happen without input.
    #[must_use]
    pub fn next_wake(&self) -> Option<Duration> {
        let mut timer = self.platform.until_next();
        if self.dom.borrow().anim.borrow().active() {
            timer = Some(timer.map_or(FRAME, |t| t.min(FRAME)));
        }
        if self.pollers.borrow().is_empty() {
            return timer;
        }
        Some(timer.map_or(POLL, |t| t.min(POLL)))
    }

    /// Moves the clock the transitions run on forward (for tests, and for a still at a chosen moment of one).
    pub fn advance_animations(&self, by: Duration) {
        let dom = self.dom.borrow();
        dom.anim.borrow_mut().advance(by);
    }

    /// A channel into the views from other threads: what is sent (from any thread) is handled on the thread of the
    /// views, in order, at the next [`tick`](Self::tick).
    ///
    /// ```ignore
    /// let tx = renderer.channel(move |catalog: Vec<Row>| rows.set(catalog));
    /// std::thread::spawn(move || tx.send(fetch()));
    /// ```
    pub fn channel<T: Send + 'static>(&self, handler: impl Fn(T) + 'static) -> mpsc::Sender<T> {
        let (tx, rx) = mpsc::channel::<T>();
        let handler = Rc::new(handler);
        self.pollers.borrow_mut().push(Rc::new(move || {
            let mut any = false;
            while let Ok(message) = rx.try_recv() {
                handler(message);
                any = true;
            }
            any
        }));
        tx
    }
}
