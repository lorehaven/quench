---
title: API
parent: quench-log-macros
grand_parent: Crates
nav_order: 1
---

## `#[instrument]`

```rust
#[proc_macro_attribute]
pub fn instrument(attr: TokenStream, item: TokenStream) -> TokenStream
```

Runs a function inside a span named after it, with its arguments recorded as `Debug` fields. Applies to a free function or method (`ItemFn`; the item must parse as a function).

```rust
#[quench_log::instrument]
async fn publish(name: String, version: u32) { /* .. */ }

#[quench_log::instrument(skip(db, body), fields(size = body.len()), level = "debug")]
async fn upload(db: &Db, body: Vec<u8>) { /* .. */ }
```

### Options

Comma-separated inside the attribute parentheses:

| Option | Meaning |
|---|---|
| `skip(a, b)` | leave these arguments out (use for non-`Debug` or secret values). `self` can be skipped; a name bound inside a pattern argument (`Inject(config): Inject<Config>`) can be skipped too. A name that is not an argument is an error |
| `skip_all` | leave out every argument |
| `fields(name = expr, other = %expr)` | extra fields appended to the span, same syntax as `span!` (`%` Display, `?` Debug) |
| `level = "debug"` | span level: `trace`, `debug`, `info`, `warn`, `error` (any case); default `info` |
| `name = "other"` | span name; default the function's name |

### Expansion

* Each argument with a plain identifier pattern is recorded as `ident = ?ident` (Debug). **`self` is recorded too** (`self = ?self`) unless skipped; patterns that are not a single identifier are not recorded.
* The span is `::quench_log::span!(::quench_log::Level::<L>, "<name>", <args...> <fields>,)`.
* A plain `fn`: `let __quench_span = span; let __quench_entered = __quench_span.enter(); <original block>`.
* An `async fn`: `let __quench_span = span; ::quench_log::Instrument::instrument(async move <original block>, __quench_span).await`, so the span is entered on every poll and survives `.await` and task migration.

### Compile errors

| Condition | Message |
|---|---|
| unknown option | ``unsupported #[instrument] option `x`; supported: skip, skip_all, fields, level, name `` |
| `level` not one of the five | `level must be one of trace, debug, info, warn, error` |
| `skip` names something that is not an argument | `` `x` is not an argument of this function `` |
| item is not a function / attribute does not parse | the `syn` parse error |

### Pitfalls

* Arguments must implement `Debug` unless skipped.
* The span exists only if the level/target is enabled; otherwise it is inert (`Span::none()`).
* `fields(...)` expressions are evaluated inside the span construction, which happens only when enabled.
