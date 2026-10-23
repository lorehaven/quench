//! Where events go: a [`Dispatcher`] holds the filter and the sinks. One is
//! installed process-wide; tests scope another to a closure.

use std::cell::RefCell;
use std::fmt;
use std::io::{IsTerminal, Write};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use quench_core::time::Time;

use crate::appender::NonBlocking;
use crate::config::Format;
use crate::field::FieldValue;
use crate::format;
use crate::level::{Filter, Level};
use crate::record::Record;
use crate::span::Span;

/// Receives every event that passes the filter.
pub trait Sink: Send + Sync {
    fn record(&self, record: &Record);
}

/// A filter and the sinks it feeds.
pub struct Dispatcher {
    filter: Filter,
    sinks: Vec<Box<dyn Sink>>,
}

impl fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Dispatcher")
            .field("filter", &self.filter)
            .field("sinks", &self.sinks.len())
            .finish()
    }
}

impl Dispatcher {
    #[must_use]
    pub fn new(filter: Filter) -> Self {
        Self {
            filter,
            sinks: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_sink(mut self, sink: impl Sink + 'static) -> Self {
        self.sinks.push(Box::new(sink));
        self
    }

    #[must_use]
    pub fn enabled(&self, level: Level, target: &str) -> bool {
        !self.sinks.is_empty() && self.filter.enabled(level, target)
    }

    pub fn dispatch(&self, record: &Record) {
        for sink in &self.sinks {
            sink.record(record);
        }
    }
}

/// Plain text to standard output, coloured when that is a terminal.
#[derive(Debug)]
pub struct ConsoleSink {
    ansi: bool,
}

impl ConsoleSink {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ansi: std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none(),
        }
    }
}

impl Default for ConsoleSink {
    fn default() -> Self {
        Self::new()
    }
}

impl Sink for ConsoleSink {
    fn record(&self, record: &Record) {
        let line = format::text(record, self.ansi);
        let _ = std::io::stdout().lock().write_all(line.as_bytes());
    }
}

/// Lines to a [`NonBlocking`] file writer, as text or JSON.
#[derive(Debug)]
pub struct FileSink {
    format: Format,
    queue: NonBlocking,
}

impl FileSink {
    #[must_use]
    pub const fn new(format: Format, queue: NonBlocking) -> Self {
        Self { format, queue }
    }
}

impl Sink for FileSink {
    fn record(&self, record: &Record) {
        let line = match self.format {
            Format::Json => format::json(record),
            Format::Plain => format::text(record, false),
        };
        self.queue.send(line.into_bytes());
    }
}

/// Keeps records in memory; for tests that assert on what was logged.
#[derive(Debug, Clone, Default)]
pub struct MemorySink {
    records: Arc<Mutex<Vec<Record>>>,
}

impl MemorySink {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything logged so far.
    #[must_use]
    pub fn records(&self) -> Vec<Record> {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Sink for MemorySink {
    fn record(&self, record: &Record) {
        self.records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(record.clone());
    }
}

impl Sink for Arc<dyn Sink> {
    fn record(&self, record: &Record) {
        (**self).record(record);
    }
}

/// A dispatcher is already installed for the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlreadySet;

impl fmt::Display for AlreadySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a log dispatcher is already installed for this process")
    }
}

impl std::error::Error for AlreadySet {}

static GLOBAL: OnceLock<Dispatcher> = OnceLock::new();

thread_local! {
    static SCOPED: RefCell<Vec<Arc<Dispatcher>>> = const { RefCell::new(Vec::new()) };
}

/// Installs the process-wide dispatcher; it can only be done once.
pub fn set_global(dispatcher: Dispatcher) -> Result<(), AlreadySet> {
    GLOBAL.set(dispatcher).map_err(|_| AlreadySet)
}

/// Whether anything is installed as the process-wide dispatcher.
#[must_use]
pub fn global_is_set() -> bool {
    GLOBAL.get().is_some()
}

/// Runs `f` with `dispatcher` receiving this thread's events, then restores
/// the previous one. Never touches process-wide state, so tests using it do
/// not interfere with each other.
pub fn with_default<R>(dispatcher: Dispatcher, f: impl FnOnce() -> R) -> R {
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            SCOPED.with(|stack| {
                stack.borrow_mut().pop();
            });
        }
    }
    SCOPED.with(|stack| stack.borrow_mut().push(Arc::new(dispatcher)));
    let _pop = Pop;
    f()
}

fn with_current<R>(f: impl FnOnce(&Dispatcher) -> R) -> Option<R> {
    let scoped = SCOPED.with(|stack| stack.borrow().last().cloned());
    match scoped {
        Some(dispatcher) => Some(f(&dispatcher)),
        None => GLOBAL.get().map(f),
    }
}

/// Is an event at `level` from `target` wanted by whatever is installed?
#[must_use]
pub fn enabled(level: Level, target: &str) -> bool {
    with_current(|d| d.enabled(level, target)).unwrap_or(false)
}

/// Builds the record (timestamp, enclosing spans) and sends it on. The macros
/// call this after checking [`enabled`].
pub fn dispatch_event(
    level: Level,
    target: &str,
    message: String,
    fields: Vec<(&'static str, FieldValue)>,
) {
    let record = Record {
        time: Time::now(),
        level,
        target: target.to_string(),
        message,
        fields,
        spans: Span::snapshot(),
    };
    with_current(|d| d.dispatch(&record));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatcher(level: Level) -> (Dispatcher, MemorySink) {
        let sink = MemorySink::new();
        (
            Dispatcher::new(Filter::level(level)).with_sink(sink.clone()),
            sink,
        )
    }

    #[test]
    fn scoped_dispatchers_receive_events_and_nest() {
        let (outer, outer_sink) = dispatcher(Level::Info);
        let (inner, inner_sink) = dispatcher(Level::Debug);
        with_default(outer, || {
            dispatch_event(Level::Info, "t", "to outer".into(), vec![]);
            with_default(inner, || {
                assert!(enabled(Level::Debug, "t"));
                dispatch_event(
                    Level::Debug,
                    "t",
                    "to inner".into(),
                    vec![("k", FieldValue::U64(1))],
                );
            });
            assert!(!enabled(Level::Debug, "t"));
            dispatch_event(Level::Info, "t", "outer again".into(), vec![]);
        });
        let outer_messages: Vec<_> = outer_sink
            .records()
            .into_iter()
            .map(|r| r.message)
            .collect();
        assert_eq!(outer_messages, ["to outer", "outer again"]);
        let inner_records = inner_sink.records();
        assert_eq!(inner_records.len(), 1);
        assert_eq!(inner_records[0].fields, [("k", FieldValue::U64(1))]);
        assert_eq!(inner_records[0].target, "t");
    }

    #[test]
    fn the_scope_unwinds_on_panic() {
        let (d, _) = dispatcher(Level::Info);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            with_default(d, || panic!("boom"))
        }));
        assert!(result.is_err());
        SCOPED.with(|stack| assert!(stack.borrow().is_empty()));
    }

    #[test]
    fn a_dispatcher_with_no_sinks_enables_nothing() {
        let empty = Dispatcher::new(Filter::level(Level::Trace));
        assert!(!empty.enabled(Level::Error, "t"));
        assert!(format!("{empty:?}").contains("sinks"));
    }

    #[test]
    fn the_filter_is_applied_per_target() {
        let sink = MemorySink::new();
        let d = Dispatcher::new(Filter::parse("warn,loud=trace").unwrap()).with_sink(sink);
        assert!(d.enabled(Level::Trace, "loud::x"));
        assert!(!d.enabled(Level::Info, "quiet"));
        assert!(d.enabled(Level::Warn, "quiet"));
    }

    #[test]
    fn nothing_installed_means_nothing_enabled() {
        // (runs on a thread with no scoped dispatcher; the global one is set by
        // other tests in this binary only through `set_global`, which these don't call)
        std::thread::spawn(|| {
            if !global_is_set() {
                assert!(!enabled(Level::Error, "t"));
                dispatch_event(Level::Error, "t", "dropped".into(), vec![]);
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn sinks_can_be_shared() {
        let sink = MemorySink::new();
        let shared: Arc<dyn Sink> = Arc::new(sink.clone());
        let d = Dispatcher::new(Filter::level(Level::Info)).with_sink(shared);
        with_default(d, || {
            dispatch_event(Level::Info, "t", "via arc".into(), vec![]);
        });
        assert_eq!(sink.records()[0].message, "via arc");
    }

    #[test]
    fn already_set_displays() {
        assert!(AlreadySet.to_string().contains("already installed"));
    }
}
