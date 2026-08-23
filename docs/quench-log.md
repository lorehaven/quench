# Quench Log

`quench-log` (crate `quench_log`) is Forge's one `tracing` subscriber: plain-text to the console, rolling JSON (by default) to disk, both at once, from a `Config` that reads the same handful of environment variables everywhere. It replaces the `tracing_subscriber::fmt().init()` each service used to hand-roll for itself. `quench-starter::logging::init()` delegates here; anything that does not want the rest of `quench-starter` (`foundry-service`, a standalone CLI) can depend on this crate alone.

## Public API / Key Types

- **`init()`** — reads `Config::from_env()` and installs it as the process's global default subscriber. What every service's `main` calls. Panics if a subscriber is already installed, or the log directory can't be created — either means the caller's own setup is wrong, and proceeding with only one sink (or none) would mean a run whose logs are silently incomplete.
- **`init_with(config: Config)`** — as `init()`, from a `Config` the caller built itself.
- **`try_init_with(config: Config) -> Result<(), Error>`** — as `init_with`, returning the error instead of panicking.
- **`subscriber(config: &Config) -> Result<(impl Subscriber + Send + Sync, Option<WorkerGuard>), Error>`** — builds the same subscriber `init`/`try_init_with` would install, without installing it. For a caller that wants to layer something else on top (OpenTelemetry, say) before calling `tracing::subscriber::set_global_default` itself, or for a test that wants to scope a subscriber to one closure via `tracing::subscriber::with_default` instead of touching process-global state — useful since a real global default can only be installed once per process, and a whole test binary shares it. The returned `WorkerGuard` is `None` when `config.file_enabled` is `false`; keep it alive for as long as logging needs to happen, since dropping it shuts the background writer down and flushes what's left.
- **`Config`** — `console_enabled`, `file_enabled` (both default `true`), `level` (an `EnvFilter` directive string, default `"info"`), `file_dir` (default `"logs"`, created if missing), `file_prefix` (default `"app"`), `file_format: Format` (default `Json`), `rotation: Rotation` (default `Daily`), `max_files: usize` (default `7`; `0` means unlimited). `Config::from_env()` reads the environment variables below, falling back to `Config::default()` for whatever is unset or unparseable.
- **`Format`** — `Json` (default) or `Plain`. Only the file sink's format is a choice; console output is always plain text. `parse`/`as_str`/`Display`.
- **`Rotation`** — `Daily` (default), `Hourly`, or `Never` (one file, nothing ever pruned). `parse`/`as_str`/`Display`.
- **`Error`** — `CreateLogDir`, `BuildAppender`, `AlreadyInitialized` (`thiserror`-derived).

## Configuration

Read by `Config::from_env()`:

- `LOG_CONSOLE_ENABLED` / `LOG_FILE_ENABLED` — `true`/`false` and the usual spellings (`1`/`0`, `yes`/`no`, `on`/`off`, case-insensitive).
- `RUST_LOG` — the same variable every other `tracing` setup in the estate already reads, so turning this crate on changes no habits. Applies to both sinks; there is deliberately no way to give the file a different level than the console.
- `LOG_DIR` — directory the rolling file(s) are written into.
- `LOG_FILE_PREFIX` — prepended to each file's name, so several services sharing one `LOG_DIR` don't overwrite each other's logs. Falls back to Cargo's own `CARGO_BIN_NAME` (set for whichever binary of a multi-binary crate is actually running), then to `"app"`.
- `LOG_FILE_FORMAT` — `json` or `plain`/`text`.
- `LOG_ROTATION` — `daily`, `hourly`, or `never`.
- `LOG_MAX_FILES` — how many rotated files to keep before the oldest is deleted; `0` for unlimited.

## Design notes

- The file sink writes on a background thread (`tracing-appender`'s non-blocking writer), so a burst of logging never stalls the caller on disk I/O. Its `WorkerGuard` has to outlive the writes it flushes; `init`/`init_with`/`try_init_with` store it in a process-wide static rather than handing it back, since every existing call site expects a plain `fn init()` with nothing to hold onto.
- The console and file layers are boxed and collected into one `Vec` before being added with a single `.with()` call, rather than one `.with()` per layer — each `.with()` changes the subscriber's own concrete type, so two sinks added that way would need boxing against two different types; `Vec<L>` implements `Layer` wherever `L` does, so a list added in one call needs only one.

## Testing

`quench-log/tests/unit.rs` bundles `config_tests` (pure parsing/default-value logic — no env var to set and restore) and `init_tests` (real logging behavior: JSON/plain output, file naming, directory creation, sink toggles). Since a global default subscriber can only be installed once per process, every `init_tests` case except one builds a subscriber via `subscriber()` and scopes it to the test with `tracing::subscriber::with_default` rather than calling `init`/`try_init_with` — the one exception exists specifically to check that a second `try_init_with` call fails once a subscriber is installed.

## Usage example

```rust
fn main() {
    quench_log::init(); // reads RUST_LOG and friends - see Config::from_env
    tracing::info!("ready");
}
```

A caller that wants its own `Config` instead of `Config::from_env()`:

```rust
use quench_log::{Config, Format, Rotation};

quench_log::init_with(Config {
    file_dir: "/var/log/my-service".into(),
    file_format: Format::Plain,
    rotation: Rotation::Hourly,
    max_files: 24,
    ..Config::default()
});
```

[Home](../README.md)
