---
title: Internals
parent: quench-core
grand_parent: Crates
nav_order: 4
---

Design notes that matter to users of `quench-core`.

## The environment setters and the workspace's `unsafe`

`env::set` and `env::remove` wrap `std::env::set_var` / `remove_var`, which are `unsafe` in edition 2024. Each function carries `#[allow(unsafe_code)]` and a `SAFETY:` comment in `quench-core/src/env.rs`; the crate otherwise inherits `unsafe_code = "deny"` from the workspace. The file is listed in `scripts/unsafe-allowlist.txt`.

The contract (stated in the doc comment of `env::set`, kept by callers, not checkable by the code):

* **Startup:** `dotenv::load` / `dotenv::load_from` and the `.env` loader of `quench-starter` run first thing in `main`, before the program starts a thread.
* **Tests:** the harness runs tests on threads, so a test that sets variables must use names nothing else reads. Library code reads the environment only when asked.
* **Otherwise:** pass settings as values; for a child process set the variable on the `Command`.

## The registry constructor

`submit!` places an `extern "C" fn` pointer in the platform's init-array section with `#[used]` and `#[cfg_attr(..., unsafe(link_section = "..."))]` (`.init_array`, `__DATA,__mod_init_func`, `.CRT$XCU`). The attribute spelling `unsafe(link_section)` is required by edition 2024, so the expansion contains the `unsafe` keyword even though no `unsafe` block exists. Constructors run in link order before `main`; `Registry::iter` returns newest first (the reverse of the order constructors ran). If the linker never pulls an object file in, its registrations do not exist. The macro has no `wasm32` gate in the source.

## wasm32

`std` has no clock or entropy there. The host calls once at start-up:

* `quench_core::time::set_clock(fn() -> i128)`: nanoseconds since the Unix epoch. Until set, `Time::now()` is the epoch (the code uses `map_or(0, ...)`).
* `quench_core::random::set_source(fn(&mut [u8]) -> bool)`: fills the buffer, returns success. Until set, `random::try_fill` fails; `random::fill` and `bytes` then panic.

`getrandom` is not a dependency on `wasm32`.

## Representation and limits

* `Time` and `Duration` store `i128` nanoseconds; arithmetic does not overflow in practice. `num_seconds` / `num_milliseconds` saturate to the `i64` range.
* `Date` stores days since 1970-01-01 as `i64`; month arithmetic clamps the day.
* `ShardedMap` has 16 shards chosen by a `RandomState` hash; `len`, `retain`, `for_each` are not atomic across shards.
* `Walk` keeps a stack of unvisited children per open directory (a directory listing is read in full when it is entered). Link-loop detection runs only with `follow_links(true)`.
* `FuturesUnordered` polls every pending future on every poll; it is meant for a handful of tasks.
* `block_on` uses a thread-parking waker; it is not a general executor.
* Base64 decoding is canonical-only (see `NonCanonical`).

## Why the environment is process-wide

`.env` loading has to change the real process environment because services and libraries read their settings with `std::env::var`. The old documentation records two rejected designs: an in-process overlay (every `std::env` reader would stay blind to `.env` values) and a re-exec after loading (restarts the process). The `env`/`dotenv` unit tests check that a variable set is seen by `std::env::var`, `var`, `var_os` and the getters, that replacing and removing work, that a child process (`sh -c`) inherits what was set, that a non-UTF-8 value survives for `var_os` and is hidden from `var`, and that a loaded `.env` never overrides a variable that was already set.

## Verification claims carried over (not re-run here)

The old page states that decoding was checked against the `base64` crate on random and mutated input, version requirements against `semver` on thousands of generated requirements, `shell::split` against `shlex::split` on 300,000 random inputs, directory walks against `walkdir` on real trees, and that `dotenv` differs from `dotenvy` only on `dotenvy`'s own bugs (`$A_B` expanded as `$A` plus `_B`, `"$X"` rejected). Those comparison harnesses are not part of the repository.
