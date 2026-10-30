---
title: API reference
parent: quench-android
grand_parent: Crates
nav_order: 2
---

The crate root (`src/lib.rs`) and `ops` (`src/ops.rs`). `serve` is a private module whose `run` and `serve` are re-exported; its `MAX_FRAME` constant is `pub` but not re-exported, so it cannot be named from outside the crate.

## Crate root

### `type Handler`

```rust
pub type Handler = fn(&[u8]) -> Result<Vec<u8>, String>;
```

An operation: bytes in, bytes or a message out. A plain `fn` pointer: a non-capturing closure coerces to it, a capturing one does not.

### `register`

```rust
pub fn register(op: &'static str, handler: Handler)
```

Makes `handler` answer calls to `op`. Registering a name again replaces the earlier handler. Safe to call from any thread (the registry is a `Mutex<HashMap>`; a poisoned lock is recovered).

### `call`

```rust
pub fn call(op: &str, input: &[u8]) -> Result<Vec<u8>, String>
```

Registers the built-in operations once, then runs `op`.

Errors:

* `no native operation `<op>``: the name is not registered.
* the handler's own message.
* ``native operation `<op>` panicked: <message>``: the handler panicked (caught with `catch_unwind`; the message is the payload when it is a `&str` or `String`, otherwise `panic`).

### `encode`

```rust
#[must_use]
pub fn encode(result: Result<Vec<u8>, String>) -> Vec<u8>
```

The bytes of an answer frame's payload: `0` followed by the result, or `1` followed by the message's UTF-8 bytes. The length prefix is added by `serve`.

### `serve`

```rust
pub fn serve<R: Read, W: Write>(input: &mut R, output: &mut W) -> io::Result<()>
```

Answers requests until the input ends cleanly. For each request it reads two frames (the operation name, the input), runs `call`, and writes one frame (`encode` of the result), flushing after each. A name that is not UTF-8 answers the error `the operation name is not text`.

Errors (`io::Error`): a frame that ends inside its 4-byte length (`UnexpectedEof`, "a frame ended inside its length"); a frame larger than 64 MiB (`InvalidData`, "a frame of N bytes is over the limit"); an operation frame with no input frame after it (`UnexpectedEof`, "a request has no input frame"); a body shorter than its length; an answer larger than `u32::MAX` bytes ("the answer is too large"); any error of the writer. End of input at a frame boundary is `Ok(())`.

### `run`

```rust
#[must_use]
pub fn run(init: fn()) -> ExitCode
```

The program's `main`: calls `init` (where operations are registered), then `serve` on locked standard input and output. Returns `ExitCode::SUCCESS` when the input ends, otherwise prints `quench native: <error>` to standard error and returns `ExitCode::FAILURE`.

## `ops`

### Built-in operations

Registered by `call` on first use (and therefore by `run`):

| Operation | Input | Answer |
|---|---|---|
| `core.version` | ignored | this crate's version as text (`env!("CARGO_PKG_VERSION")`, `0.1.0`). |
| `core.echo` | any bytes | the same bytes. |
| `tz.next` | JSON `{"zone": string, "time": "HH:MM", "after_ms": integer}` | JSON `{"at_ms": integer}`. |

`tz.next` errors (as the message of a failure answer): input not UTF-8 or not JSON (the parser's message), `` `zone` is missing ``, `` `time` is missing ``, `` `after_ms` is missing ``, `` `time` is HH:MM ``, plus the errors of `next_local`.

### `ops::next_local`

```rust
pub fn next_local(zone: &str, hour: u32, minute: u32, after_ms: i64) -> Result<i64, String>
```

The first instant strictly after `after_ms` (milliseconds since the epoch) at which the clock in `zone` reads `hour:minute`. It examines up to four local dates starting with the zone's date at `after_ms`.

* A reading that happens twice (clocks go back) fires the first time it is reached (the earlier candidate is taken if it is after `after_ms`, otherwise the later).
* A reading that never happens (clocks go forward) is taken as if the clock had not moved: the offset in force 12 hours earlier is applied, so it fires an hour later on the clock.

Errors: ``HH:MM is not a time of day`` (hour above 23 or minute above 59, formatted `{hour:02}:{minute:02}`), `` no time zone `<zone>` `` (the name does not parse as a `quench_tz::Zone`), `the date is out of range`, `no occurrence in the next days`.
