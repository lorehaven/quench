//! What the UI needs from the host beyond drawing: a clock, one-shot timers, and the user's
//! motion preference. The browser backend installs its own; [`ManualPlatform`] is one a test
//! drives by hand.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// The services a host provides. Install one with [`install`].
pub trait Platform {
    /// Milliseconds on a monotonic clock.
    fn now(&self) -> f64;
    /// Runs `f` once after `ms` milliseconds, unless cancelled. Returns an id for [`Platform::cancel`].
    fn after(&self, ms: u32, f: Box<dyn FnOnce()>) -> u32;
    fn cancel(&self, id: u32);
    /// Whether the user asked for less motion (`prefers-reduced-motion: reduce`).
    fn prefers_reduced_motion(&self) -> bool {
        false
    }
    /// Asks the person to pick files (`accept` is a comma-separated list of extensions or media types, as in an HTML file
    /// input) and calls `done` with them, or with none when they cancel. It must be called while handling a click or
    /// another action of the person, or the browser will not show the picker. The default picks nothing.
    fn pick_files(&self, accept: &str, multiple: bool, done: Box<dyn FnOnce(Vec<PickedFile>)>) {
        let _ = (accept, multiple);
        done(Vec::new());
    }
    /// Offers `bytes` to the person as a file to save. The default does nothing.
    fn download(&self, name: &str, mime: &str, bytes: &[u8]) {
        let _ = (name, mime, bytes);
    }
}

/// A file the person picked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickedFile {
    pub name: String,
    /// The media type the host reports; may be empty.
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// Asks the person to pick files; `done` gets them, or an empty list on cancel. With no platform installed it is called at
/// once with none. See [`Platform::pick_files`] for when it may be called.
pub fn pick_files(accept: &str, multiple: bool, done: impl FnOnce(Vec<PickedFile>) + 'static) {
    match current() {
        Some(p) => p.pick_files(accept, multiple, Box::new(done)),
        None => done(Vec::new()),
    }
}

/// Offers `bytes` to the person as a file called `name` to save. Nothing happens with no platform installed.
pub fn download(name: &str, mime: &str, bytes: &[u8]) {
    if let Some(p) = current() {
        p.download(name, mime, bytes);
    }
}

/// Asks to be told when the person drops files on the page. One handler is kept (a later one replaces it).
pub fn on_drop(handler: impl Fn(Vec<PickedFile>) + 'static) {
    DROP_HANDLER.with(|h| *h.borrow_mut() = Some(Rc::new(handler)));
}

/// A host calls this with the files the person dropped. Does nothing when nobody asked to hear of them.
pub fn deliver_drop(files: Vec<PickedFile>) {
    let handler = DROP_HANDLER.with(|h| h.borrow().clone());
    if let Some(h) = handler {
        h(files);
    }
}

/// Shows `bytes` (a PDF, an image, a page) in a new browser tab instead of saving it.
///
/// The person can read or print it there. It is a download with no name, which a host that saves files may take as "show it". Call it while handling a click.
pub fn open_in_tab(mime: &str, bytes: &[u8]) {
    download("", mime, bytes);
}

type DropHandler = Rc<dyn Fn(Vec<PickedFile>)>;

thread_local! {
    static DROP_HANDLER: RefCell<Option<DropHandler>> = const { RefCell::new(None) };
    static PLATFORM: RefCell<Option<Rc<dyn Platform>>> = const { RefCell::new(None) };
}

/// Makes `platform` the one the UI uses on this thread.
pub fn install(platform: Rc<dyn Platform>) {
    PLATFORM.with(|p| *p.borrow_mut() = Some(platform));
}

fn current() -> Option<Rc<dyn Platform>> {
    PLATFORM.with(|p| p.borrow().clone())
}

/// Whether a platform is installed on this thread (without one, [`after`] runs its function at once).
#[must_use]
pub fn installed() -> bool {
    current().is_some()
}

/// The clock, or 0 when no platform is installed.
#[must_use]
pub fn now() -> f64 {
    current().map_or(0.0, |p| p.now())
}

/// A timer that can be cancelled. Dropping it does not cancel it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer(u32);

impl Timer {
    pub fn cancel(self) {
        if let Some(p) = current() {
            p.cancel(self.0);
        }
    }
}

/// Runs `f` after `ms` milliseconds. With no platform installed it runs at once, so a UI built
/// without a host (a server render) still reaches its final state.
pub fn after(ms: u32, f: impl FnOnce() + 'static) -> Timer {
    if let Some(p) = current() {
        Timer(p.after(ms, Box::new(f)))
    } else {
        f();
        Timer(0)
    }
}

/// Whether to skip animations.
#[must_use]
pub fn prefers_reduced_motion() -> bool {
    current().is_some_and(|p| p.prefers_reduced_motion())
}

type Pending = (u32, f64, Box<dyn FnOnce()>);
type PendingPick = (String, bool, Box<dyn FnOnce(Vec<PickedFile>)>);

/// A platform whose clock and timers move only when told to.
pub struct ManualPlatform {
    now: Cell<f64>,
    next: Cell<u32>,
    reduced: Cell<bool>,
    timers: RefCell<Vec<Pending>>,
    picks: RefCell<Vec<PendingPick>>,
    downloads: RefCell<Vec<(String, String, Vec<u8>)>>,
}

impl std::fmt::Debug for ManualPlatform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManualPlatform")
            .field("now", &self.now.get())
            .field("reduced", &self.reduced.get())
            .field("timers", &self.timers.borrow().len())
            .finish_non_exhaustive()
    }
}

impl Default for ManualPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualPlatform {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            now: Cell::new(0.0),
            next: Cell::new(1),
            reduced: Cell::new(false),
            timers: RefCell::new(Vec::new()),
            picks: RefCell::new(Vec::new()),
            downloads: RefCell::new(Vec::new()),
        }
    }

    /// The pickers waiting for an answer, oldest first: `(accept, multiple)`.
    #[must_use]
    pub fn pending_picks(&self) -> Vec<(String, bool)> {
        self.picks
            .borrow()
            .iter()
            .map(|(a, m, _)| (a.clone(), *m))
            .collect()
    }

    /// Answers the oldest picker with these files (an empty list is a cancel). `false` when none is waiting.
    pub fn answer_pick(&self, files: Vec<PickedFile>) -> bool {
        let first = {
            let mut picks = self.picks.borrow_mut();
            (!picks.is_empty()).then(|| picks.remove(0))
        };
        first.is_some_and(|(_, _, done)| {
            done(files);
            true
        })
    }

    /// The files offered for saving, oldest first: `(name, mime, bytes)`.
    #[must_use]
    pub fn downloads(&self) -> Vec<(String, String, Vec<u8>)> {
        self.downloads.borrow().clone()
    }

    /// Moves the clock forward, running the timers that fall due, earliest first (including those
    /// they start that fall due within the same span).
    pub fn advance(&self, ms: f64) {
        let target = self.now.get() + ms;
        loop {
            let due = {
                let mut timers = self.timers.borrow_mut();
                let next = timers
                    .iter()
                    .enumerate()
                    .filter(|(_, (_, at, _))| *at <= target)
                    .min_by(|a, b| a.1.1.total_cmp(&b.1.1))
                    .map(|(i, _)| i);
                next.map(|i| timers.remove(i))
            };
            let Some((_, at, f)) = due else { break };
            self.now.set(at.max(self.now.get()));
            f();
        }
        self.now.set(target);
    }

    /// Timers that have not run.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.timers.borrow().len()
    }

    pub fn set_reduced_motion(&self, on: bool) {
        self.reduced.set(on);
    }
}

impl Platform for ManualPlatform {
    fn now(&self) -> f64 {
        self.now.get()
    }

    fn after(&self, ms: u32, f: Box<dyn FnOnce()>) -> u32 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.timers
            .borrow_mut()
            .push((id, self.now.get() + f64::from(ms), f));
        id
    }

    fn cancel(&self, id: u32) {
        self.timers.borrow_mut().retain(|(i, _, _)| *i != id);
    }

    fn prefers_reduced_motion(&self) -> bool {
        self.reduced.get()
    }

    fn pick_files(&self, accept: &str, multiple: bool, done: Box<dyn FnOnce(Vec<PickedFile>)>) {
        self.picks
            .borrow_mut()
            .push((accept.to_string(), multiple, done));
    }

    fn download(&self, name: &str, mime: &str, bytes: &[u8]) {
        self.downloads
            .borrow_mut()
            .push((name.to_string(), mime.to_string(), bytes.to_vec()));
    }
}
