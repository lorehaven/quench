# quench-test-bdd-macros
The step attributes and World derive of `quench-test-bdd`.

## What it is
`quench-test-bdd-macros` is the proc-macro crate behind `quench-test-bdd`: the step attributes `#[given]`, `#[when]`, `#[then]` and the derive `World`. Use them through `quench-test-bdd`, which re-exports them; the generated code names `::quench_test_bdd::..`.

A step takes `&mut World` first, then one parameter per capture (any `FromStr` type), and may be `async`. A parameter of type `&Step` receives the step and consumes no capture.

## Use cases
- Bind Gherkin steps to Rust functions with cucumber expressions.
- Bind steps with a regular expression via `regex = "..."`.
- Derive `World` from `Default` for per-scenario state.
- Build the world with an async constructor via `#[world(init = path)]`.

## Example
```rust
use quench_test_bdd::{Step, World, given, then, when};

#[derive(World)]
#[world(init = Browser::launch)]
struct Browser { pages: i64 }
impl Browser { async fn launch() -> Self { Browser { pages: 0 } } }

#[given("a user {string}")]
fn user(world: &mut Browser, name: String) { world.pages = name.len() as i64; }

#[when(regex = r"^I read the table$")]
fn table(world: &mut Browser, step: &Step) {
    world.pages = step.table().expect("a table").rows.len() as i64;
}

#[then("there are {int} pages")]
fn pages(world: &mut Browser, n: i64) { assert_eq!(world.pages, n); }
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-test-bdd-macros/
