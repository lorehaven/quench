//! Test cases: what `#[test]` registers, and how a test body becomes an outcome.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt::Debug;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

/// What a test is expected to do about panicking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShouldPanic {
    No,
    Yes,
    /// Must panic with a message containing this text.
    WithMessage(&'static str),
}

/// How a test body ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    /// It returned an `Err`.
    Failed(String),
    /// It panicked, with the message and location.
    Panicked(String),
}

/// A registered test.
#[derive(Debug)]
pub struct TestCase {
    /// Module path and function name.
    pub name: &'static str,
    pub file: &'static str,
    pub line: u32,
    /// `Some(reason)` when the test is ignored; the reason may be empty.
    pub ignore: Option<&'static str>,
    pub should_panic: ShouldPanic,
    pub run: fn() -> Outcome,
}

/// Every test registered by `#[test]` in the binary.
pub static TESTS: quench_core::registry::Registry<TestCase> =
    quench_core::registry::Registry::new();

thread_local! {
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
    static LOCATION: RefCell<Option<String>> = const { RefCell::new(None) };
    static CURRENT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Makes panics on this thread silent and remembers where they happened; the runner prints them with the
/// failure report instead.
pub fn capture_panics(test: &str) {
    CAPTURING.with(|c| c.set(true));
    CURRENT.with(|c| *c.borrow_mut() = test.to_string());
}

/// While true, panics on any thread are silent (their location is still recorded).
static SILENT: AtomicBool = AtomicBool::new(false);
static HOOK: Once = Once::new();

/// Installs the panic hook that cooperates with [`capture_panics`] and [`silence_panics`]; other threads
/// keep the default behaviour. Installed once.
pub fn install_hook() {
    HOOK.call_once(|| {
        let default = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            if CAPTURING.with(Cell::get) || SILENT.load(Ordering::Relaxed) {
                let location = info
                    .location()
                    .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
                LOCATION.with(|slot| *slot.borrow_mut() = location);
            } else {
                default(info);
            }
        }));
    });
}

/// Turns the silencing of panics on every thread on or off (for runners that run work on a thread pool and
/// report failures themselves). Use [`take_panic_location`] right after catching a panic.
pub fn silence_panics(on: bool) {
    if on {
        install_hook();
    }
    SILENT.store(on, Ordering::Relaxed);
}

/// Where the last panic on this thread happened (`file:line:column`), once.
pub fn take_panic_location() -> Option<String> {
    LOCATION.with(|slot| slot.borrow_mut().take())
}

/// The text of a panic payload.
#[must_use]
pub fn panic_message(payload: &(dyn Any + Send)) -> String {
    payload_text(payload)
}

fn payload_text(payload: &(dyn Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Box<dyn Any>".to_string()
    }
}

pub fn panicked(payload: &(dyn Any + Send)) -> Outcome {
    let message = payload_text(payload);
    let location = LOCATION.with(|slot| slot.borrow_mut().take());
    let name = CURRENT.with(|c| c.borrow().clone());
    Outcome::Panicked(match location {
        Some(location) => format!("thread '{name}' panicked at {location}:\n{message}"),
        None => format!("thread '{name}' panicked:\n{message}"),
    })
}

/// What a test function may return.
pub trait Termination {
    /// `Ok` to pass, `Err` with the text to fail.
    ///
    /// # Errors
    ///
    /// The failure message of an `Err` return.
    fn finish(self) -> Result<(), String>;
}

impl Termination for () {
    fn finish(self) -> Result<(), String> {
        Ok(())
    }
}

impl<E: Debug> Termination for Result<(), E> {
    fn finish(self) -> Result<(), String> {
        self.map_err(|e| format!("Error: {e:?}"))
    }
}

fn outcome_of<R: Termination>(result: std::thread::Result<R>) -> Outcome {
    match result {
        Ok(value) => match value.finish() {
            Ok(()) => Outcome::Passed,
            Err(message) => Outcome::Failed(message),
        },
        Err(payload) => panicked(payload.as_ref()),
    }
}

/// Used by the code `#[test]` generates.
#[doc(hidden)]
pub mod private {
    use super::{AssertUnwindSafe, Future, Outcome, Termination, outcome_of, panic};

    pub fn run_sync<R: Termination>(body: impl FnOnce() -> R) -> Outcome {
        outcome_of(panic::catch_unwind(AssertUnwindSafe(body)))
    }

    pub fn run_async<R: Termination, F: Future<Output = R>>(
        multi_thread: bool,
        worker_threads: Option<usize>,
        body: impl FnOnce() -> F,
    ) -> Outcome {
        let built = if multi_thread {
            let mut builder = tokio::runtime::Builder::new_multi_thread();
            if let Some(n) = worker_threads {
                builder.worker_threads(n);
            }
            builder.enable_all().build()
        } else {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
        };
        let runtime = match built {
            Ok(runtime) => runtime,
            Err(e) => return Outcome::Failed(format!("could not start a runtime: {e}")),
        };
        outcome_of(panic::catch_unwind(AssertUnwindSafe(|| {
            runtime.block_on(body())
        })))
    }
}
