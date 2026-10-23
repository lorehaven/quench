# Quench Log

`quench-log` (crate `quench_log`) is the logging every service and CLI in the estate uses: leveled events with fields, spans, plain-text console output and a rolling JSON (by default) file, both at once, from a `Config` that reads the same handful of environment variables everywhere. It replaces `tracing`, `tracing-subscriber` and `tracing-appender` with a few hundred lines of its own; the only third-party-free dependencies are `quench-core` (time, errors), `quench-serde` (the JSON lines) and the [`#[instrument]`](./quench-log-macros.md) attribute. `quench-starter::logging::init()` delegates here; anything that does not want the rest of `quench-starter` (`foundry-service`, a standalone CLI) can depend on this crate alone.

## Writing logs

```rust
fn main() {
    quench_log::init(); // reads RUST_LOG and friends - see Config::from_env
    quench_log::info!(port = 8080, "ready");
}
```

`error!`, `warn!`, `info!`, `debug!` and `trace!` (and `event!` with an explicit level, or `target: "audit"` to set the target) take fields first, then an optional format string:

```rust
quench_log::warn!(attempts = 3, user = %name, "locked out: {reason}");
quench_log::debug!(?request, "incoming");
quench_log::info!(user, "signed in"); // a bare `user` records the variable of that name
```

`name = value` records numbers, booleans and strings as they are, `name = %value` uses `Display`, `name = ?value` uses `Debug`. The target defaults to the module path and is what `RUST_LOG` directives match against.

### Spans

A `Span` carries fields that ride along on every event logged inside it. `span!(Level::Info, "handle", id = 7)` (or `info_span!`, `debug_span!`, ...) creates one; `span.enter()` returns a guard for synchronous code, `span.in_scope(|| ..)` runs a closure inside it, and `span.record("status", 200)` fills in a field later. For futures, `fut.instrument(span)` enters the span on every poll, so it survives `.await`s and task migration. `Span::current()` is the innermost entered span.

`#[quench_log::instrument]` does that for a whole function: it records the arguments as `Debug` fields and instruments `async fn`s as futures. Options are in [Quench Log Macros](./quench-log-macros.md).

## Setting up

- **`init()`** — `init_with(Config::from_env())`. What every service's `main` calls. Panics if a dispatcher is already installed or the log directory can't be created: either means the caller's own setup is wrong, and carrying on with one sink (or none) would mean a run whose logs are silently incomplete.
- **`init_console()`** — console only, at the environment's level, no log file and no directory. For examples and small tools.
- **`init_with(config)`** / **`try_init_with(config) -> Result<(), Error>`** — from a `Config` the caller built, panicking or returning the error.
- **`subscriber(&Config) -> Result<(Dispatcher, Option<Guard>), Error>`** — builds what `init` would install, without installing it. For a caller that wants to add its own `Sink` before `set_global(dispatcher)`, or for a test that scopes a dispatcher to one closure with `with_default(dispatcher, || ..)` instead of touching the one process-wide global a whole test binary shares. The `Guard` is `None` when the file sink is off; keep it alive while logging needs to happen, since dropping it shuts the background writer down after flushing what is left.
- **`Config`** — `console_enabled`, `file_enabled` (both default `true`), `level` (a `RUST_LOG`-style directive string, default `"info"`), `file_dir` (default `"logs"`, created if missing), `file_prefix` (default `"app"`), `file_format: Format` (default `Json`), `rotation: Rotation` (default `Daily`), `max_files: usize` (default `7`; `0` means unlimited). `Config::from_env()` reads the variables below, falling back to `Config::default()` for whatever is unset or unparseable.
- **`Format`** — `Json` (default) or `Plain`. Only the file's format is a choice; console output is always plain text, in colour on a terminal. **`Rotation`** — `Daily` (default), `Hourly`, or `Never` (one file, nothing pruned).
- **`Error`** — `CreateLogDir`, `BuildAppender`, `AlreadyInitialized`.

## Filtering

`RUST_LOG`-style directives: `info`, `warn,quench_http=debug`, `info,hyper=off`. The most specific matching target prefix wins; a bare level sets the default; an unparseable string falls back to `info`. `Filter::parse` and `Filter::enabled(level, target)` are public for tools that want the same rules.

## Output

Plain text: `2026-10-23T12:00:00.123456Z  INFO handle{id=7}: my_crate::module: message k=v`, one line per event, spans outermost first. JSON: one object per line with `timestamp`, `level`, `target`, `message`, the event's fields at the top level, and a `spans` array. Timestamps are UTC with microseconds.

## Configuration

Read by `Config::from_env()`:

- `LOG_CONSOLE_ENABLED` / `LOG_FILE_ENABLED` — `true`/`false` and the usual spellings (`1`/`0`, `yes`/`no`, `on`/`off`, case-insensitive).
- `RUST_LOG` — the directive string. Applies to both sinks; there is deliberately no way to give the file a different level than the console.
- `LOG_DIR` — directory the rolling file(s) are written into.
- `LOG_FILE_PREFIX` — prepended to each file's name, so several services sharing one `LOG_DIR` don't overwrite each other's logs. Falls back to Cargo's `CARGO_BIN_NAME`, then to `"app"`.
- `LOG_FILE_FORMAT` — `json` or `plain`/`text`.
- `LOG_ROTATION` — `daily`, `hourly`, or `never`.
- `LOG_MAX_FILES` — how many rotated files to keep before the oldest is deleted; `0` for unlimited.

## Design notes

- The file sink writes on a background thread behind a bounded queue (`non_blocking`), so a burst of logging never stalls the caller on disk I/O. The `Guard` has to outlive the writes it flushes; `init`/`init_with`/`try_init_with` keep it in a process-wide static rather than handing it back, since every call site expects a plain `fn init()`.
- A `Dispatcher` is a `Filter` plus a list of `Sink`s (`ConsoleSink`, `FileSink`, `MemorySink` for tests). The global one is set once; `with_default` shadows it for the current thread, which is how tests capture output without interfering with each other.
- Span fields are snapshotted into each `Record`, so a sink never holds a lock on live span state.

## Testing

Unit tests live next to each module (levels and filters, field conversion, text and JSON formatting, the rolling appender with rotation and pruning, span nesting and `instrument`ed futures) and `tests/unit` covers the macros and the real file behaviour: JSON and plain output, file naming, directory creation, sink toggles. Since a global dispatcher can only be installed once per process, every case except one builds a dispatcher via `subscriber()` and scopes it with `with_default`; the exception checks that a second `try_init_with` fails.

[Home](../README.md)
