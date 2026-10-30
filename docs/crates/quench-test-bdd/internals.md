---
title: Internals
parent: quench-test-bdd
grand_parent: Crates
nav_order: 5
---

* **Step lookup**: each `#[given/when/then]` registers a `Definition` in the static `STEPS`. `Runner::run` keeps those whose `world()` TypeId equals `TypeId::of::<W>()`, compiles each pattern once (`Matcher::new`), and for every step collects the definitions of the same kind whose matcher accepts the text: none is `undefined step`, more than one is `ambiguous step`.
* **Type erasure**: a step runs against `&mut (dyn Any + Send)` and downcasts to its own world type; a mismatch (impossible through the runner) is `step run against the wrong World type`.
* **Scenario tasks**: `tokio::task::JoinSet` plus a `Semaphore`; a scenario future is guarded by `quench_core::future::catch_unwind` and `quench_test::silence_panics(true)` is on for the whole run so that panics do not print to stderr; their location is attached to the failure message.
* **Cucumber expressions** are translated to a regex piece by piece: literal characters are escaped with `quench_regex::escape`, `(opt)` becomes `(?:opt)?`, `a/b` becomes `(?:a|b)`, parameters become capture groups (`{string}` has two alternatives, so its group index advances by two).
* **Failure reports**: console output lists each failed scenario as `Feature > scenario  (file:line)`, its failed steps and their messages; the report cases keep every step with its status and duration. Steps after a failed one are `skipped`.
* **Outline location** is the line of the `Scenario Outline:` for every row.
* **Limits**: concurrency 64 by default; no timeout per step or scenario; hooks (`Before`/`After`) do not exist: use the world constructor and `Drop`.
