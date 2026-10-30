---
title: Usage
parent: quench-log
grand_parent: Crates
nav_order: 1
---

## 1. Initialise once, early

```rust
fn main() {
    quench_log::init();            // Config::from_env(): console + rolling JSON file
    quench_log::info!("started");
}
```

`init()` panics if a dispatcher is already installed or the log directory cannot be created. Use `init_console()` for examples and small tools (no file, no directory), `init_with(config)` for a hand-built `Config`, or `try_init_with(config)` to get the `Result`:

```rust
use quench_log::{Config, Format, Rotation};
let config = Config { file_format: Format::Plain, rotation: Rotation::Hourly, max_files: 24, ..Config::default() };
quench_log::try_init_with(config)?;
```

The file writer's `Guard` is kept in a process-wide static by these functions.

## 2. Events with fields

```rust
use quench_log::{info, warn, debug};
let name = "ada"; let reason = "too many tries"; let user = 7_u32;
warn!(attempts = 3, user = %name, "locked out: {reason}");
debug!(?name, "incoming");
info!(user, "signed in");                     // bare name records the variable `user`
info!(target: "audit", action = "login", "ok"); // explicit target
quench_log::event!(quench_log::Level::Info, "runtime level");
```

`name = value` records numbers, booleans and strings as typed values; `%value` uses `Display`; `?value` uses `Debug`. `Option<T>` fields with `None` record nothing. Pitfall: arguments are evaluated only if the level and target are enabled.

## 3. Spans

```rust
use quench_log::{Level, span, field::Empty};
let span = span!(Level::Info, "handle", id = 7, status = Empty);
let _guard = span.enter();           // events inside carry id=7
quench_log::info!("working");
span.record("status", 200);          // fill the declared field later
```

`span.in_scope(|| ..)` runs a closure inside it; `Span::current()` returns the innermost entered span. Level-named forms exist: `trace_span!` ... `error_span!`.

## 4. Spans across `.await`

```rust
use quench_log::Instrument;
async fn handler() {
    async { quench_log::info!("inside"); }
        .instrument(quench_log::info_span!("request", id = 1))
        .await;
}
```

`instrument` enters the span on every poll, so it survives task migration. A guard from `enter()` must not be held across an `.await`.

## 5. `#[instrument]`

```rust
#[quench_log::instrument(skip(db, body), fields(size = body.len()), level = "debug")]
async fn upload(db: &Db, body: Vec<u8>) { /* .. */ }
```

Arguments are recorded as `Debug` fields, `self` included unless skipped. See [quench-log-macros](../quench-log-macros/api.md).

## 6. Test what was logged

```rust
use quench_log::{Dispatcher, Filter, Level, MemorySink};
let sink = MemorySink::new();
quench_log::with_default(Dispatcher::new(Filter::level(Level::Trace)).with_sink(sink.clone()), || {
    quench_log::warn!(code = 500, "boom");
});
let records = sink.records();
assert_eq!(records[0].level, Level::Warn);
```

## 7. Custom sink

```rust
use quench_log::{Record, Sink};
struct Stderr;
impl Sink for Stderr {
    fn record(&self, record: &Record) { eprint!("{}", quench_log::format_text(record, false)); }
}
let (dispatcher, guard) = quench_log::subscriber(&quench_log::Config::default())?;
quench_log::set_global(dispatcher.with_sink(Stderr))?;
// keep `guard` alive while logging should reach the file
```
