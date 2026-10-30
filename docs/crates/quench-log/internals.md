---
title: Internals
parent: quench-log
grand_parent: Crates
nav_order: 4
---

## Pipeline

1. A macro (`info!` and friends, `event!`) checks `enabled(level, target)` first. Only if true does it evaluate the message and field expressions, build the field vector and call `dispatch_event`. Disabled events cost the check only.
2. `dispatch_event` builds a `Record` (time from `Time::now()`, target, message, fields, a `Span::snapshot()` of the entered spans) and hands it to the current dispatcher: the innermost `with_default` scope of this thread, else the process-wide one, else nothing.
3. A `Dispatcher` feeds each `Sink` in order. `Dispatcher::enabled` is also false when no sink is attached.

## Global and scoped dispatchers

`set_global` uses a `OnceLock`: the first install wins, a second returns `Err(AlreadySet)`. `with_default` pushes a dispatcher on a thread-local stack for the duration of a closure (popped by a drop guard, also on panic). Threads other than the caller do not see the scoped one.

## Spans

A thread-local stack of `Arc<Inner>` holds the entered spans. `Span::enter` pushes, dropping the `Entered` guard pops. Span fields are copied into every `Record` (`SpanSnapshot`, outermost first, empty fields skipped), so sinks never hold span locks. A span created while its level is disabled is `Span::none()`, which is inert everywhere. `Instrumented` boxes the future (so it is `Unpin`) and enters the span around each `poll`.

## File sink

`non_blocking` starts a thread `quench-log-writer` reading from a bounded `sync_channel`. `NonBlocking::send` uses `try_send`: a full queue drops the line and increments a counter. Dropping the `Guard` sends a shutdown message (blocking if the queue is full) and joins the thread, which flushes. `init`/`try_init_with` keep the guard in a process-wide `OnceLock`; the guard of a failed install is dropped (and thus flushed) with it.

## Output formats

Text: `TIMESTAMP LEVEL ` where `LEVEL` is right-aligned to 5 columns (coloured with ANSI codes when `ansi`), then each enclosing span as `name{k=v k=v}: `, then `target:`, then ` message` if any, then ` name=value` per non-empty field, then a newline. String field values print Debug-quoted; `%`/`?` fields print as rendered.

JSON: one object per line:

```json
{"timestamp":"2026-10-23T12:00:00.123456Z","level":"INFO","fields":{"message":"ready","port":8080},"target":"my_crate","span":{"id":7,"name":"handle"},"spans":[{"id":7,"name":"handle"}]}
```

`fields` holds `message` (when non-empty) followed by the event fields; `span` (the innermost) and `spans` (outermost first) appear only inside spans, each span object holding its fields and `name`. Timestamps are UTC with microseconds. The console uses colour only when stdout is a terminal and `NO_COLOR` is unset.

## Limits

`u128`/`i128` field values saturate to 64 bits; `isize`/`usize` convert with saturation. There is no per-sink level. `Level` ordering is `Trace < Debug < Info < Warn < Error`.
