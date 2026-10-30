---
title: Testing
parent: quench-test
grand_parent: Crates
nav_order: 8
---

`quench-test` tests itself with its own harness: unit tests in `runner.rs` (command-line parsing, output of a run with fake `TestCase`s, filters, ignored tests, listing, quiet mode, the help text), `predicate.rs`, `report`, and four integration targets (all `harness = false`):

| Target | Covers |
|---|---|
| `dogfood` | the macros and runner end to end: sync, async (current and multi thread), `Result` returns, `should_panic`, `ignore`, module paths, locations |
| `cmd` | `Command`, `Assert`, timeouts, stdin, environment |
| `mock` | the mock server: matchers, priorities, expectations, bodies, delays, keep-alive |
| `report` | the model, JSON round trip, HTML, merge, index, report directory rules |

```bash
cargo test -p quench-test
```
