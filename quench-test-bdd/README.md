# quench-test-bdd
A light Gherkin test runtime: feature-file parser, cucumber expressions, async steps, concurrent scenarios, JSON and HTML reports.

## What it is
`quench-test-bdd` is a light Gherkin runtime. You write `.feature` files, give their steps Rust functions with `#[given]`, `#[when]` and `#[then]`, and run them with a `Runner`. Scenarios run concurrently (those tagged `@serial` one by one afterwards), a failed step skips the rest of its scenario, and results are printed and written as JSON and HTML reports through `quench_test::report`. It replaces `cucumber` and `gherkin`.

The step attributes and `#[derive(World)]` come from `quench-test-bdd-macros` and are re-exported. The report model comes from `quench-test`.

## Use cases
- Run `.feature` files as acceptance tests.
- Write async steps with cucumber expressions or regexes.
- Read doc strings and tables from steps with `&Step`.
- Filter by tag expression or scenario name with `Options`.
- Keep per-scenario state in a `World`.

## Example
```rust
use quench_test_bdd::{World, given, then, when};

#[derive(World, Default)]
struct Calc { total: i64 }

#[given("a total of {int}")]
fn start(world: &mut Calc, n: i64) { world.total = n; }

#[when(regex = r"^I multiply by (\d+)$")]
async fn multiply(world: &mut Calc, n: i64) { world.total *= n; }

#[then("the total is {int}")]
fn check(world: &mut Calc, n: i64) -> Result<(), String> {
    if world.total == n { Ok(()) } else { Err(format!("total is {}", world.total)) }
}

#[tokio::main]
async fn main() {
    let summary = quench_test_bdd::Runner::<Calc>::new()
        .title("calc")
        .run("tests/features")
        .await;
    std::process::exit(i32::from(summary.failed()));
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-test-bdd/
