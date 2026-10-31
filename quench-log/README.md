# quench-log
Leveled logging with fields and spans: plain-text console plus rolling JSON file logs.

## What it is
`quench-log` is the logging used by every service and CLI of the estate: leveled events with fields, spans, plain-text console output, and a rolling file (JSON by default), both at once, configured from a handful of environment variables. It replaces `tracing`, `tracing-subscriber` and `tracing-appender`.

The `#[instrument]` attribute comes from `quench-log-macros` and is re-exported. `quench-starter` delegates its logging setup to this crate, so a program that wants nothing else of it can depend on `quench-log` alone.

## Use cases
- Initialise console and file logging with one `init()` call.
- Record events with typed, `Display` or `Debug` fields.
- Wrap work in spans, including across `.await` with `Instrument`.
- Trace functions with `#[instrument]`.
- Assert on log records in tests with `MemorySink`.

## Example
```rust
use quench_log::Instrument;

#[quench_log::instrument(skip(body), level = "debug")]
async fn upload(body: Vec<u8>) { /* .. */ }

#[tokio::main]
async fn main() {
    quench_log::init(); // reads RUST_LOG and the LOG_* variables
    let name = "ada";
    quench_log::info!(port = 8080, user = %name, "ready");
    async { quench_log::info!("inside"); }
        .instrument(quench_log::info_span!("request", id = 1))
        .await;
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-log/
