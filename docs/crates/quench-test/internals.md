---
title: Internals
parent: quench-test
grand_parent: Crates
nav_order: 7
---

## Registration

`#[quench_test::test]` leaves the function in place and adds a `const _: () = { ... }` that calls `quench_core::registry::submit!(TESTS, TestCase, TestCase { ... })`. The registry is a link-time collection from `quench_core::registry`; there is no global constructor on the crate's own part, and no runtime state besides the registered values. `main!` reads it through `run_registered()`.

## Execution

* The selected tests are sorted by name and pulled from a shared atomic counter by `min(threads, number of tests)` scoped threads; `threads` is `--test-threads` or `available_parallelism`.
* Before running, `install_hook` sets a panic hook that, on a thread that called `capture_panics` (all test threads) or while `silence_panics(true)` is on, stores the `file:line:column` and prints nothing; other threads use the previous hook. The failure text is `thread '<name>' panicked at <location>:\n<message>`.
* A sync test runs inside `catch_unwind`; an async test builds a Tokio runtime (`new_current_thread` or `new_multi_thread` with optional `worker_threads`, always `enable_all`), `block_on`s it inside `catch_unwind`. If the runtime cannot be built the test fails with `could not start a runtime: <error>`.
* Printing is serialised by a mutex; ignored tests print first, in name order.
* Test output is not captured; this is why `--nocapture` is accepted and ignored.

## Reports

The runner makes one `Suite` (kind `unit`) of all cases in the binary; the title is `<CARGO_PKG_NAME> · <binary name>` with the 16-hex-digit hash Cargo appends to test executables stripped. Files are written atomically.

## Mock server

One OS thread per server with a current-thread Tokio runtime; each connection is a task that loops reading requests (keep-alive), so tests can use any HTTP client. State (rules, call counts, received requests) is behind a `Mutex`; a poisoned lock is recovered. Rule choice: order by `priority` (stable), first rule that is not exhausted (`up_to_n_times`) and whose matchers all hold; its call count is incremented. Shutdown: dropping the server sends on a oneshot that stops the accept loop (open connections finish on their own).

## Limits

Request bodies over 64 MiB are refused (the connection is closed). Response delays use `tokio::time::sleep` on the server thread. The mock speaks HTTP/1.1 only, no TLS.
