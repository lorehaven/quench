---
title: Usage
parent: quench-test-bdd
grand_parent: Crates
nav_order: 1
---

## 1. A world, steps and a runner

The `World` is the state of one scenario; a fresh one is made per scenario. The derive uses `Default`, or an async constructor:

```rust,ignore
use quench_test_bdd::{World, given, then, when};

#[derive(World, Default)]
struct Calc { total: i64, words: Vec<String> }

#[derive(World)]
#[world(init = Browser::launch)]
struct Browser { /* ... */ }
impl Browser { async fn launch() -> Self { Browser {} } }
```

Steps are free functions whose first parameter is `&mut World`; the other parameters take the captures in order (any `FromStr` type). A step may be `async`, and may return `()` or `Result<(), E>` with `E: Display` (an `Err` fails the step); a panic also fails it:

```rust,ignore
#[given("a total of {int}")]
fn start(world: &mut Calc, n: i64) { world.total = n; }

#[when(regex = r"^I multiply by (\d+)$")]
async fn multiply(world: &mut Calc, n: i64) { world.total *= n; }

#[then("the total is {int}")]
fn check(world: &mut Calc, n: i64) -> Result<(), String> {
    if world.total == n { Ok(()) } else { Err(format!("total is {}", world.total)) }
}
```

## 2. Run from a test target

With the standard unit-test harness replaced (`[[test]] name = "bdd" harness = false`):

```rust,ignore
#[tokio::main]
async fn main() {
    let summary = quench_test_bdd::Runner::<Calc>::new()
        .title("calc")
        .run("tests/features")
        .await;
    std::process::exit(i32::from(summary.failed()));
}
```

`run` takes a file or a directory (searched recursively for `*.feature`, sorted by path) and returns a `Summary`. Exit non-zero on `summary.failed()`.

Alternatively run it inside a `#[quench_test::test]` (see the repository's `quench-test-bdd/tests/features.rs`): build a multi-thread Tokio runtime and `block_on` the runner; the unit test's report and the BDD report are both written.

## 3. Doc strings and tables

```rust,ignore
use quench_test_bdd::Step;

#[when("I read the doc string")]
fn doc(world: &mut Calc, step: &Step) {
    world.words.push(step.docstring().expect("a doc string").clone());
}

#[when("I read the table")]
fn table(world: &mut Calc, step: &Step) {
    let rows = &step.table().expect("a table").rows;
    world.total = rows.len() as i64;
}
```

A parameter of type `&Step` (any path ending in `Step`) receives the step itself and is not counted among the captures.

## 4. Choose what runs

```rust,ignore
use quench_test_bdd::{Options, Runner};

let options = Options { tags: Some("@fast and not @slow".into()), name: Some("^add".into()), ..Options::default() };
let runner = Runner::<Calc>::new()
    .options(options)
    .filter(|feature, _rule, scenario| feature.name != "Experimental" && !scenario.name.contains("wip"));
```

`Options` has no parser of its own; flatten it into your own command line with `quench-args`: [Building command-line tools](../../guides/building-cli-tools.md), and [configuration](configuration.md).

## 5. Concurrency and `@serial`

Scenarios run concurrently up to `concurrency` (default 64) on the Tokio runtime you call `run` from. A scenario tagged `@serial` (on the feature, rule, scenario or its `Examples`) runs alone after all the others. Give worlds their own resources (ports, temp directories) or tag the scenarios that share them `@serial`.

## Pitfalls

* An unmatched step fails the scenario with `` undefined step: `When ...` ``; two matching definitions fail it with `ambiguous step ... matches file:line, file:line`. Matching is per step kind: a `#[given]` never matches a `When` step.
* An `And` step takes the kind of the step before it (a leading `And` after `Background:` counts as `Given`).
* Step definitions are global (`STEPS`), but a runner only uses those whose world type is its own `W`.
* `run` **panics** on an invalid `--tags` expression or `--name` regex and on a step pattern that does not compile (with the file and line of the step).
