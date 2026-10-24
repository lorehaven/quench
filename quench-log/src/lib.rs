//! Logging for every service and CLI in the estate: leveled events with
//! fields, spans, plain-text console output, and a rolling JSON (by default)
//! file, both at once - without third-party crates.
//!
//! ```no_run
//! quench_log::init(); // reads RUST_LOG and friends - see `Config::from_env`
//! quench_log::info!(port = 8080, "ready");
//! ```
//!
//! # Events and spans
//!
//! [`info!`] and friends take optional fields and a message:
//! `warn!(attempts = 3, user = %name, "locked out: {reason}")`. A [`Span`]
//! carries fields that ride along on every event logged inside it; hold its
//! `enter()` guard in synchronous code, or wrap a future with
//! [`Instrument::instrument`] so it is entered on every poll. The
//! `#[instrument]` attribute does that for a whole function.
//!
//! # Filtering and sinks
//!
//! `RUST_LOG`-style directives (`info,quench_http=debug,hyper=warn`) decide
//! what is logged; [`Config`] says where it goes. A file sink writes on a
//! background thread, so a burst of logging never stalls the caller on disk
//! I/O. Its [`Guard`] has to outlive the writes it flushes; `init` stores it
//! in a process-wide static rather than handing it back, since every existing
//! call site expects a plain `fn init()` with nothing to hold onto.
//! [`try_init_with`] and [`subscriber`] are the lower-level entry points for
//! a caller that wants the guard itself, or a `Result` instead of a panic.

mod appender;
pub mod config;
mod dispatch;
pub mod field;
mod format;
mod level;
mod macros;
mod record;
mod span;

pub use appender::{Guard, NonBlocking, RollingWriter, non_blocking};
pub use config::{Config, Format, Rotation};
pub use dispatch::{
    AlreadySet, ConsoleSink, Dispatcher, FileSink, MemorySink, Sink, dispatch_event, enabled,
    global_is_set, set_global, with_default,
};
pub use field::FieldValue;
pub use format::{json as format_json, text as format_text, timestamp};
pub use level::{Filter, FilterError, Level, LevelFilter};
pub use record::{Record, SpanSnapshot};
pub use span::{Entered, Instrument, Instrumented, Span};

pub use quench_log_macros::instrument;

use std::path::PathBuf;
use std::sync::OnceLock;

/// Holds the file sink's background-writer guard for the life of the
/// process. Set at most once: a second `init()` in the same process is a
/// caller bug (see [`Error::AlreadyInitialized`]), not a reason to leak a
/// second worker thread silently.
static GUARD: OnceLock<Guard> = OnceLock::new();

#[derive(Debug, quench_core::Error)]
pub enum Error {
    #[error("could not create log directory {path}: {source}")]
    CreateLogDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not set up the rolling file appender: {0}")]
    BuildAppender(#[from] std::io::Error),
    // The process-wide dispatcher can be set exactly once. A second call -
    // two `init()`s, or `init()` after something else already installed one -
    // hits this rather than silently doing nothing or replacing what is
    // already capturing logs.
    #[error("a log dispatcher is already installed for this process")]
    AlreadyInitialized,
}

/// Installs logging from `RUST_LOG` and the other variables `Config::from_env` reads.
///
/// This is what every service's `main` calls; `init_with` and
/// `try_init_with` exist for a caller that wants to build its own `Config`
/// or handle the error itself (tests included, since a whole test binary
/// panicking on the second `#[quench_test::test]` to call `init()` would be worse than
/// unhelpful).
///
/// # Panics
///
/// Panics if a dispatcher is already installed, or the log directory
/// can't be created. Both mean the caller's own setup is wrong - proceeding
/// with only one sink, or none, would mean a run whose logs are silently
/// incomplete, which is worse than refusing to start.
pub fn init() {
    init_with(Config::from_env());
}

/// Console-only logging for examples and small tools: the environment's
/// level, no log file (so no directory is created).
pub fn init_console() {
    init_with(Config {
        file_enabled: false,
        ..Config::from_env()
    });
}

/// As [`init`], but from a `Config` the caller built itself rather than
/// `Config::from_env()`.
pub fn init_with(config: Config) {
    if let Err(err) = try_init_with(config) {
        panic!("quench-log: {err}");
    }
}

/// As [`init_with`], returning the error instead of panicking on it.
#[allow(clippy::needless_pass_by_value)] // public API takes the config by value, like `init_with`
pub fn try_init_with(config: Config) -> Result<(), Error> {
    let (built, guard) = subscriber(&config)?;

    set_global(built).map_err(|_| Error::AlreadyInitialized)?;

    // Only stashed once the dispatcher actually won the race above - the
    // guard of a losing attempt is dropped with it instead, which flushes
    // whatever that attempt would have written rather than leaking it.
    if let Some(guard) = guard {
        let _ = GUARD.set(guard);
    }

    Ok(())
}

/// Builds the dispatcher `init`/`try_init_with` install, without installing it.
///
/// For a caller that wants to add its own sink before calling [`set_global`]
/// itself, and for tests, which use [`with_default`] to check what actually
/// reaches a file rather than fighting over the one global a whole test
/// binary shares.
///
/// The returned [`Guard`] is `None` when `config.file_enabled` is `false` -
/// there is no background writer to flush in that case. Keep it alive for as
/// long as logging needs to happen; dropping it shuts the writer down and
/// flushes what is left, which is also why a caller using this directly
/// should not drop it before installing the dispatcher.
pub fn subscriber(config: &Config) -> Result<(Dispatcher, Option<Guard>), Error> {
    let filter = Filter::parse(&config.level).unwrap_or_else(|_| Filter::level(Level::Info));
    let mut dispatcher = Dispatcher::new(filter);
    let mut guard = None;

    if config.console_enabled {
        dispatcher = dispatcher.with_sink(ConsoleSink::new());
    }
    if config.file_enabled {
        std::fs::create_dir_all(&config.file_dir).map_err(|source| Error::CreateLogDir {
            path: config.file_dir.clone(),
            source,
        })?;
        let writer = RollingWriter::new(
            &config.file_dir,
            &config.file_prefix,
            "log",
            config.rotation,
            config.max_files,
        )?;
        let (queue, file_guard) = non_blocking(writer);
        dispatcher = dispatcher.with_sink(FileSink::new(config.file_format, queue));
        guard = Some(file_guard);
    }

    Ok((dispatcher, guard))
}

#[cfg(test)]
quench_test::main!();
