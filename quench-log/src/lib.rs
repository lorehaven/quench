//! One `tracing` subscriber for every service and CLI in the estate:
//! plain-text to the console, rolling JSON (by default) to disk, both at
//! once. Replaces the `tracing_subscriber::fmt().init()` every service used
//! to hand-roll for itself - `quench-starter::logging::init()` delegates
//! here, and anything that does not want the rest of `quench-starter`
//! (`foundry-service`, a standalone CLI) can depend on this crate alone.
//!
//! ```no_run
//! quench_log::init(); // reads RUST_LOG and friends - see `Config::from_env`
//! tracing::info!("ready");
//! ```
//!
//! A file sink writes on a background thread (`tracing-appender`'s
//! non-blocking writer), so a burst of logging never stalls the caller on
//! disk I/O. Its `WorkerGuard` has to outlive the writes it flushes; `init`
//! stores it in a process-wide static rather than handing it back, since
//! every existing call site expects a plain `fn init()` with nothing to
//! hold onto. `try_init_with` is the lower-level entry point for a caller
//! that wants the guard itself, or wants a `Result` instead of a panic.

pub mod config;

pub use config::{Config, Format, Rotation};

use std::path::PathBuf;
use std::sync::OnceLock;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, prelude::*};

/// Holds the file sink's background-writer guard for the life of the
/// process. Set at most once: a second `init()` in the same process is a
/// caller bug (see [`Error::AlreadyInitialized`]), not a reason to leak a
/// second worker thread silently.
static GUARD: OnceLock<WorkerGuard> = OnceLock::new();

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not create log directory {path}: {source}")]
    CreateLogDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not set up the rolling file appender: {0}")]
    BuildAppender(#[from] tracing_appender::rolling::InitError),
    /// `tracing_subscriber`'s global subscriber can be set exactly once per
    /// process. A second call - two `init()`s, or `init()` after something
    /// else already installed one - hits this rather than silently doing
    /// nothing or replacing what is already capturing logs.
    #[error("a tracing subscriber is already installed for this process")]
    AlreadyInitialized,
}

/// Installs logging from `RUST_LOG` and the other variables `Config::from_env`
/// reads. This is what every service's `main` calls; `init_with` and
/// `try_init_with` exist for a caller that wants to build its own `Config`
/// or handle the error itself (tests included, since a whole test binary
/// panicking on the second `#[test]` to call `init()` would be worse than
/// unhelpful).
///
/// # Panics
///
/// Panics if a subscriber is already installed, or the log directory
/// can't be created. Both mean the caller's own setup is wrong - proceeding
/// with only one sink, or none, would mean a run whose logs are silently
/// incomplete, which is worse than refusing to start.
pub fn init() {
    init_with(Config::from_env());
}

/// As [`init`], but from a `Config` the caller built itself rather than
/// `Config::from_env()`.
pub fn init_with(config: Config) {
    if let Err(err) = try_init_with(config) {
        panic!("quench-log: {err}");
    }
}

/// As [`init_with`], returning the error instead of panicking on it.
pub fn try_init_with(config: Config) -> Result<(), Error> {
    let (built, guard) = subscriber(&config)?;

    tracing::subscriber::set_global_default(built).map_err(|_| Error::AlreadyInitialized)?;

    // Only stashed once the subscriber actually won the race above - the
    // guard of a losing attempt is dropped with it instead, which flushes
    // whatever that attempt would have written rather than leaking it.
    if let Some(guard) = guard {
        let _ = GUARD.set(guard);
    }

    Ok(())
}

/// Builds the subscriber `init`/`try_init_with` install, without installing
/// it - for a caller that wants to add its own layer on top (OpenTelemetry,
/// say) before calling `tracing::subscriber::set_global_default` itself, and
/// for this crate's own tests, which use `tracing::subscriber::with_default`
/// to check what actually reaches a file rather than fighting over the one
/// global default a whole test binary shares.
///
/// The returned `WorkerGuard` is `None` when `config.file_enabled` is
/// `false` - there is no background writer to flush in that case. Keep it
/// alive for as long as logging needs to happen; dropping it shuts the
/// writer down and flushes what is left, which is also why a caller using
/// this directly should not drop it before `set_global_default`.
pub fn subscriber(
    config: &Config,
) -> Result<
    (
        impl tracing::Subscriber + Send + Sync + use<>,
        Option<WorkerGuard>,
    ),
    Error,
> {
    let env_filter = EnvFilter::try_new(&config.level).unwrap_or_else(|_| EnvFilter::new("info"));

    // Collected into one `Vec` and added with a single `.with()` rather than
    // one `.with()` call per layer: each `.with()` changes the subscriber's
    // own concrete type, so two sinks added that way would need boxing
    // against two different types. `Vec<L>` implements `Layer` itself
    // wherever `L` does, so a list added in one call needs only one.
    let mut layers: Vec<BoxedLayer> = Vec::new();
    let mut guard = None;

    if config.console_enabled {
        layers.push(tracing_subscriber::fmt::layer().with_target(true).boxed());
    }
    if config.file_enabled {
        let (layer, file_guard) = build_file_layer(config)?;
        layers.push(layer);
        guard = Some(file_guard);
    }

    Ok((
        tracing_subscriber::registry().with(env_filter).with(layers),
        guard,
    ))
}

/// A layer whose concrete type has been erased, since the console layer
/// (plain text) and the file layer (JSON by default) are different
/// instantiations of `fmt::Layer`, and every element of the `Vec` they are
/// collected into below has to share one type.
///
/// Boxed against the subscriber type *after* the `EnvFilter` layer, not bare
/// `Registry` - `.with(env_filter)` changes the concrete type the next
/// `.with()` has to accept, and a layer boxed against the wrong one simply
/// does not implement `Layer` for it.
type BoxedLayer = Box<
    dyn Layer<tracing_subscriber::layer::Layered<EnvFilter, tracing_subscriber::Registry>>
        + Send
        + Sync,
>;

/// Builds the rolling-file layer: creates `file_dir` if it does not exist
/// yet, and wires up the non-blocking writer. The guard comes back to the
/// caller rather than being stashed here - two callers building a layer
/// each (as this crate's own tests do, deliberately never touching the
/// global default) would otherwise fight over the one process-wide slot.
fn build_file_layer(config: &Config) -> Result<(BoxedLayer, WorkerGuard), Error> {
    std::fs::create_dir_all(&config.file_dir).map_err(|source| Error::CreateLogDir {
        path: config.file_dir.clone(),
        source,
    })?;

    let mut builder = tracing_appender::rolling::Builder::new()
        .rotation(config.rotation.to_tracing())
        .filename_prefix(&config.file_prefix)
        .filename_suffix("log");
    if config.max_files > 0 {
        builder = builder.max_log_files(config.max_files);
    }
    let appender = builder.build(&config.file_dir)?;

    let (writer, guard) = tracing_appender::non_blocking(appender);

    let layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        // Files are read by `jq`/a log shipper, not a terminal - literal
        // escape codes in them would be noise at best.
        .with_ansi(false);

    let layer = if config.file_format == Format::Json {
        layer.json().boxed()
    } else {
        layer.boxed()
    };

    Ok((layer, guard))
}
