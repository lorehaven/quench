---
title: quench-test-bdd
parent: Crates
has_children: true
permalink: /crates/quench-test-bdd/
nav_order: 49
---

`quench-test-bdd` (library `quench_test_bdd`) is a light Gherkin runtime. You write `.feature` files, give their steps Rust functions with `#[given]`, `#[when]` and `#[then]`, and run them with a `Runner`. Scenarios run concurrently (those tagged `@serial` one by one afterwards), a failed step skips the rest of its scenario, and results are printed and written as JSON and HTML reports through `quench_test::report`. It replaces `cucumber` and `gherkin`.

## Facts

| | |
|---|---|
| Version | 0.1.3 |
| Edition | 2024 |
| License | MIT |
| Features | none |
| Test harness | `autotests = false`; integration tests `features` and `parsing` (`harness = false`) |

```toml
[dev-dependencies]
quench-test-bdd = { version = "0.1.3", registry = "ennor" }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Dependencies

| Crate | Why |
|---|---|
| `quench-args` | `Options` is a `quench_args::Args` group you can flatten into your own command line |
| `quench-core` | the step registry, `future::catch_unwind` |
| `quench-regex` | step regexes, cucumber expressions, `--name` |
| `quench-test` | the report model and files, panic silencing |
| `quench-test-bdd-macros` | `#[given]`, `#[when]`, `#[then]`, `#[derive(World)]` (see [quench-test-bdd-macros](../quench-test-bdd-macros/index.md)) |
| `tokio` | the scenario scheduler |

## Where to go next

* [Usage](usage.md)
* [Gherkin and expression syntax](syntax.md): every keyword, step expression and tag expression.
* [API reference](api.md)
* [Configuration](configuration.md): `Options` and the report location.
* [Internals](internals.md)
* [Testing](testing.md)
* The guide: [Testing with quench-test and quench-test-bdd](../../guides/testing.md).

## Minimal example

`tests/features/calc.feature`:

```text
Feature: Calculator
  Scenario: Adding
    Given a total of 1
    When I add 2
    Then the total is 3
```

`tests/bdd.rs` (`[[test]] name = "bdd" harness = false`):

```rust,ignore
use quench_test_bdd::{World, given, then, when};

#[derive(World, Default)]
struct Calc { total: i64 }

#[given("a total of {int}")]
fn start(world: &mut Calc, n: i64) { world.total = n; }

#[when("I add {int}")]
fn add(world: &mut Calc, n: i64) { world.total += n; }

#[then("the total is {int}")]
fn check(world: &mut Calc, n: i64) { assert_eq!(world.total, n); }

#[tokio::main]
async fn main() {
    let summary = quench_test_bdd::Runner::<Calc>::new().run("tests/features").await;
    std::process::exit(i32::from(summary.failed()));
}
```

Real output of a similar run (a failing outline row and an undefined step):

```text
scenario add ... ok
scenario outline 1 ... ok
scenario outline 2 ... FAILED
scenario unknown step ... FAILED

failures:

  Arithmetic > outline 2  (.../calc.feature:10)
    Then the total is 9
      total is 3

  Arithmetic > unknown step  (.../calc.feature:20)
    When I do something odd
      undefined step: `When I do something odd`

features: 1; scenarios: 4 (2 passed, 2 failed, 0 skipped); steps: 11 (9 passed, 2 failed, 0 skipped); finished in 0.00s
```
