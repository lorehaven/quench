---
title: API
parent: quench-log
grand_parent: Crates
nav_order: 2
---

Every public item of `quench_log`, in the order of `src/lib.rs`. `appender::period` and `appender::file_name` are `pub` but live in the private module `appender` and are not re-exported; `Config`/`Format`/`Rotation` are reachable as `quench_log::config::*` and at the root.

## Root re-exports

```rust
pub use appender::{Guard, NonBlocking, RollingWriter, non_blocking};
pub use config::{Config, Format, Rotation};
pub use dispatch::{AlreadySet, ConsoleSink, Dispatcher, FileSink, MemorySink, Sink,
                   dispatch_event, enabled, global_is_set, set_global, with_default};
pub use field::FieldValue;
pub use format::{json as format_json, text as format_text, timestamp};
pub use level::{Filter, FilterError, Level, LevelFilter};
pub use record::{Record, SpanSnapshot};
pub use span::{Entered, Instrument, Instrumented, Span};
pub use quench_log_macros::instrument;
pub mod config;  pub mod field;
```

## Setup functions and `Error`

```rust
#[derive(Debug, quench_core::Error)]
pub enum Error {
    #[error("could not create log directory {path}: {source}")]
    CreateLogDir { path: PathBuf, source: std::io::Error },
    #[error("could not set up the rolling file appender: {0}")]
    BuildAppender(#[from] std::io::Error),
    #[error("a log dispatcher is already installed for this process")]
    AlreadyInitialized,
}
pub fn init()
pub fn init_console()
pub fn init_with(config: Config)
pub fn try_init_with(config: Config) -> Result<(), Error>
pub fn subscriber(config: &Config) -> Result<(Dispatcher, Option<Guard>), Error>
```

| Function | Behaviour |
|---|---|
| `init` | `init_with(Config::from_env())`. Panics if a dispatcher is already installed or the directory cannot be created (`quench-log: {error}`) |
| `init_console` | `init_with` of `Config { file_enabled: false, ..Config::from_env() }`: no file, no directory |
| `init_with` | `try_init_with`, panicking with `quench-log: {err}` |
| `try_init_with` | builds with `subscriber`, installs with `set_global` (`AlreadyInitialized` if one exists), stores the guard in a process static only after winning the install |
| `subscriber` | builds a `Dispatcher` without installing. Filter = `Filter::parse(&config.level)` or `Filter::level(Level::Info)` if invalid. Console sink if `console_enabled`; if `file_enabled`, creates `file_dir`, a `RollingWriter` (suffix `log`), a `non_blocking` queue and a `FileSink`. The `Guard` is `None` when the file sink is off |

## `config`

```rust
pub enum Format { Json /* default */, Plain }          // Debug, Clone, Copy, PartialEq, Eq, Default; Display
impl Format { pub const fn as_str(self) -> &'static str; pub fn parse(raw: &str) -> Option<Self> }
pub enum Rotation { Daily /* default */, Hourly, Never }   // same derives; Display
impl Rotation { pub const fn as_str(self) -> &'static str; pub fn parse(raw: &str) -> Option<Self> }
pub struct Config {
    pub console_enabled: bool, pub file_enabled: bool, pub level: String,
    pub file_dir: PathBuf, pub file_prefix: String, pub file_format: Format,
    pub rotation: Rotation, pub max_files: usize,
}                                                      // Debug, Clone, PartialEq, Eq; Default
impl Config { pub fn from_env() -> Self }
#[must_use] pub fn resolve_file_prefix(explicit: Option<&str>, cargo_bin_name: Option<&str>) -> String
#[must_use] pub fn parse_bool(raw: &str) -> Option<bool>
```

* `Format::as_str`: `"json"` / `"plain"`; `parse` accepts `json`, `plain`, `text` (trimmed, case-insensitive). `Rotation::as_str`: `"daily"`/`"hourly"`/`"never"`; `parse` accepts those.
* `Config::from_env` reads the variables in [Configuration](configuration.md).
* `resolve_file_prefix`: first non-blank (trimmed) of `explicit`, `cargo_bin_name`, else `"app"`.
* `parse_bool`: `1 true yes on` to `Some(true)`, `0 false no off` to `Some(false)` (trimmed, case-insensitive), else `None`.

## `level`

```rust
pub enum Level { Trace, Debug, Info, Warn, Error }      // Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash; Display (padded)
impl Level { #[must_use] pub const fn as_str(self) -> &'static str; #[must_use] pub fn parse(text: &str) -> Option<Self> }
pub enum LevelFilter { Off, From(Level) }               // Debug, Clone, Copy, PartialEq, Eq
impl LevelFilter { #[must_use] pub fn allows(self, level: Level) -> bool }
pub struct FilterError(pub String);                     // Debug, Clone, PartialEq, Eq; Display: "invalid log filter: {0}"; Error
pub struct Filter { /* default, directives */ }         // Debug, Clone, PartialEq, Eq
impl Filter {
    #[must_use] pub const fn level(level: Level) -> Self
    pub fn parse(text: &str) -> Result<Self, FilterError>
    #[must_use] pub fn enabled(&self, level: Level, target: &str) -> bool
}
```

* Order: `Trace < Debug < Info < Warn < Error` (lower is chattier). `as_str`: `TRACE DEBUG INFO WARN ERROR`. `Level::parse`: trimmed, case-insensitive; `trace|5`, `debug|4`, `info|3`, `warn|warning|2`, `error|1`.
* `LevelFilter::allows`: `Off` never; `From(least)` when `level >= least`.
* `Filter::level(l)`: everything at `l` and above. `Filter::parse`: see [Configuration](configuration.md#filter-syntax); errors: `no directives`, or `` `x` is not a level ``. `Filter::enabled`: the first directive (longest target first) that matches wins, else the default.

## `field`

```rust
pub enum FieldValue { Bool(bool), I64(i64), U64(u64), F64(f64), Str(String), Text(String), Empty }  // Debug, Clone, PartialEq; Display
impl FieldValue { #[must_use] pub const fn is_empty(&self) -> bool }
pub trait IntoFieldValue { fn field_value(&self) -> FieldValue; }
pub struct Empty;                                        // Debug, Clone, Copy; IntoFieldValue -> FieldValue::Empty
pub fn display(value: &dyn Display) -> FieldValue        // Text(value.to_string())
pub fn debug(value: &dyn Debug) -> FieldValue            // Text(format!("{value:?}"))
```

* `Str` is a string value (text output quotes it with Debug escaping); `Text` is already rendered (`%`/`?`), printed as is; `Empty` is declared but unrecorded and never printed.
* `Display` of `FieldValue`: numbers and booleans as written, `Str` as `{:?}`, `Text` verbatim, `Empty` as nothing.
* `IntoFieldValue` implemented for: `i8 i16 i32 i64` (as `I64`), `u8 u16 u32 u64` (`U64`), `isize`/`usize` (saturating), `i128`/`u128` (saturating to 64 bits), `f32`/`f64` (`F64`), `bool`, `str`, `String`, `&T`, `Option<T>` (`None` gives `Empty`), and `Empty`.

## `record`

```rust
pub struct SpanSnapshot { pub name: &'static str, pub fields: Vec<(String, FieldValue)> }   // Debug, Clone, PartialEq
pub struct Record {
    pub time: Time, pub level: Level, pub target: String, pub message: String,
    pub fields: Vec<(&'static str, FieldValue)>,
    pub spans: Vec<SpanSnapshot>,       // enclosing spans, outermost first
}                                       // Debug, Clone, PartialEq
```

## `span`

```rust
pub struct Span { /* Option<Arc<Inner>> */ }            // Debug, Clone, Default
impl Span {
    #[must_use] pub const fn none() -> Self
    #[must_use] pub fn new(level: Level, target: &str, name: &'static str, fields: Vec<(&'static str, FieldValue)>) -> Self
    #[must_use] pub fn current() -> Self
    #[must_use] pub const fn is_none(&self) -> bool
    #[must_use = "..."] pub fn enter(&self) -> Entered<'_>
    pub fn in_scope<R>(&self, f: impl FnOnce() -> R) -> R
    pub fn record(&self, name: &str, value: impl IntoFieldValue) -> &Self
    pub fn record_display(&self, name: &str, value: &dyn Display) -> &Self
}
pub struct Entered<'a>        // Debug; Drop leaves the span
pub trait Instrument: Sized {
    fn instrument(self, span: Span) -> Instrumented<Self>
    fn in_current_span(self) -> Instrumented<Self>
}                                                       // blanket for every Future
pub struct Instrumented<F>    // Debug; Future<Output = F::Output>; Unpin
```

* `Span::new` returns `Span::none()` when `enabled(level, target)` is false; prefer the macros. A disabled span ignores `enter`, `record`, etc.
* `record` sets an existing field or appends a new one; `record_display` stores the `Display` text.
* `Entered`/`enter` keep a thread-local stack; hold the guard only in synchronous code.

## `dispatch`

```rust
pub trait Sink: Send + Sync { fn record(&self, record: &Record); }
pub struct Dispatcher { /* filter, sinks */ }           // Debug (manual)
impl Dispatcher {
    #[must_use] pub fn new(filter: Filter) -> Self
    #[must_use] pub fn with_sink(self, sink: impl Sink + 'static) -> Self
    #[must_use] pub fn enabled(&self, level: Level, target: &str) -> bool   // false with no sinks
    pub fn dispatch(&self, record: &Record)
}
pub struct ConsoleSink;   impl ConsoleSink { #[must_use] pub fn new() -> Self }   // Debug, Default
pub struct FileSink;      impl FileSink { #[must_use] pub const fn new(format: Format, queue: NonBlocking) -> Self }  // Debug
pub struct MemorySink;    impl MemorySink { #[must_use] pub fn new() -> Self; #[must_use] pub fn records(&self) -> Vec<Record> }  // Debug, Clone, Default
pub struct AlreadySet;    // Debug, Clone, Copy, PartialEq, Eq; Display: "a log dispatcher is already installed for this process"; Error
pub fn set_global(dispatcher: Dispatcher) -> Result<(), AlreadySet>
#[must_use] pub fn global_is_set() -> bool
pub fn with_default<R>(dispatcher: Dispatcher, f: impl FnOnce() -> R) -> R
#[must_use] pub fn enabled(level: Level, target: &str) -> bool
pub fn dispatch_event(level: Level, target: &str, message: String, fields: Vec<(&'static str, FieldValue)>)
```

* `ConsoleSink` writes the text format to **standard output**, in colour when stdout is a terminal and `NO_COLOR` is unset. `FileSink` formats as JSON or plain text and queues the line on a `NonBlocking`. `MemorySink` clones every record; clones share the store. `Sink` is also implemented for `Arc<dyn Sink>`.
* `with_default` scopes a dispatcher to this thread for the closure and restores the previous one; it never touches process-wide state.
* `enabled` is false when nothing is installed. `dispatch_event` stamps `Time::now()` and the span snapshot; the macros call it after `enabled`.

## `appender`

```rust
pub struct RollingWriter { /* dir, prefix, suffix, rotation, max_files, current */ }   // Debug; io::Write
impl RollingWriter { pub fn new(dir: &Path, prefix: &str, suffix: &str, rotation: Rotation, max_files: usize) -> std::io::Result<Self> }
pub struct NonBlocking { /* SyncSender, dropped counter */ }     // Debug, Clone
impl NonBlocking { pub fn send(&self, line: Vec<u8>); #[must_use] pub fn dropped(&self) -> u64 }
pub struct Guard { /* sender, worker */ }                        // Debug; Drop flushes and joins the writer thread
pub fn non_blocking<W: Write + Send + 'static>(writer: W) -> (NonBlocking, Guard)
```

`RollingWriter::new` opens (append, create) the file for the current UTC period immediately, pruning old files; each `write` rolls to a new file when the period changed. `NonBlocking::send` drops and counts the line when the queue (128,000 lines) is full. Keep the `Guard` alive while logging should reach the file.

## `format`

```rust
#[must_use] pub fn timestamp(time: Time) -> String      // 2026-10-23T12:00:00.123456Z (UTC, microseconds)
#[must_use] pub fn text(record: &Record, ansi: bool) -> String   // re-exported as format_text
#[must_use] pub fn json(record: &Record) -> String      // re-exported as format_json
```

Both formatters return a newline-terminated line. Layout in [Internals](internals.md#output-formats).

## Macros

All `#[macro_export]`, so `quench_log::info!` etc.

| Macro | Form | Notes |
|---|---|---|
| `event!` | `event!(Level::Info, ...)`, `event!(target: "t", Level::Info, ...)` | fields first, then an optional `format!`-style message |
| `trace! debug! info! warn! error!` | same arguments without the level; optional leading `target: expr,` | |
| `span!` | `span!(Level::Info, "name", id = 7, path = %p)`, `span!(target: "t", Level::Info, "name", ...)` | the name must be a literal; returns a `Span` (`Span::none()` when disabled) |
| `trace_span! debug_span! info_span! warn_span! error_span!` | span with a fixed level | |

Field forms (events and spans): `name = value`, `name = %value`, `name = ?value`; events also accept a bare `name`, `%name`, `?name` (record the variable of that name). Hidden helpers: `__event!`, `__value!`, `__span!`. `quench_log::instrument` is documented in [quench-log-macros](../quench-log-macros/api.md).
