# Quench Test BDD

`quench-test-bdd` (crate `quench_test_bdd`) runs Gherkin feature files against Rust step functions. It replaces `cucumber` (with `gherkin`, `clap` and the regex crates behind them) and writes the same JSON and HTML result pages as [`quench-test`](./quench-test.md).

## Steps and worlds

```rust
use quench_test_bdd::{Runner, Step, World, given, then, when};

#[derive(World, Default)]
struct Calc {
    total: i64,
}

#[given("a total of {int}")]
fn start(world: &mut Calc, n: i64) {
    world.total = n;
}

#[when(regex = r"^I multiply by (\d+)$")]
async fn multiply(world: &mut Calc, n: i64) {
    world.total *= n;
}

#[then("the total is {int}")]
fn check(world: &mut Calc, n: i64) -> Result<(), String> {
    (world.total == n).then_some(()).ok_or_else(|| format!("total is {}", world.total))
}

#[then("the request body is as given")]
fn body(world: &mut Calc, step: &Step) {
    let text = step.docstring().expect("a doc string");
}

#[tokio::main]
async fn main() {
    let summary = Runner::<Calc>::new().run("tests/features").await;
    std::process::exit(i32::from(summary.failed()));
}
```

- The first parameter is `&mut World`; further parameters are read from the pattern's captures in order with `FromStr`, except a `&Step` which receives the step (`docstring()`, `table()`, `value`, `line`). A step may be `async`; it returns `()` or `Result<(), E: Display>`. A panic fails the step with its location.
- `#[derive(World)]` uses `Default`, or `#[world(init = Self::new)]` for an `async fn() -> Self`. Every scenario gets a fresh world.
- Patterns are cucumber expressions — `{string}` (single or double quotes, `\"` unescaped), `{int}`, `{float}`, `{word}`, `{}`, optional `cucumber(s)`, alternation `a/an`, `\` escapes — or `regex = ".."` where every capture group is an argument. `And`, `But` and `*` take the kind of the step before. A step with no match, or with several, fails its scenario.
- Several worlds may live in one binary; each step belongs to the world of its first parameter.

## Running

`Runner::<W>::new().options(..).filter(|feature, rule, scenario| ..).title("bdd").run(path).await` reads one feature file or every `*.feature` below a directory, runs the scenarios and returns a `Summary` with `scenarios` and `steps` counts (`passed`, `failed`, `skipped`), `parse_errors`, `failed()` and the `report`.

- Scenarios run concurrently (64 at a time, `--concurrency`); those tagged `@serial` run one at a time after the rest. After a failed step the other steps of the scenario are skipped; `--fail-fast` also skips the scenarios not yet started.
- `Options` is a `quench_args::Args` group you can `#[command(flatten)]` into your own CLI: `--tags/-t "@fast and not @slow"`, `--name/-n REGEX`, `--concurrency N`, `--fail-fast`, `--report-dir DIR`, `--quiet/-q`. Tag expressions support `and`, `or`, `not` and parentheses (`quench_test_bdd::tags::Tags`).
- Tags are given to your code without the `@`, as `Scenario::tags` of the `gherkin` crate does.
- Output is one line per scenario, then the failed steps with their messages, then totals. `<title>.json`, `<title>.html` and the merged `index.html` land in the report directory of `quench-test`; the HTML page lists the features, scenarios with their tags and location, and every step with its status and error.

## Gherkin

`quench_test_bdd::gherkin::parse` / `parse_file` handle `Feature`, `Rule`, `Background`, `Scenario`/`Example`, `Scenario Outline`/`Template` with `Examples` (tagged, several per outline, `<placeholder>` substitution in names, steps, doc strings and tables), tags, comments, descriptions, doc strings (`"""` or a code fence, dedented) and tables (`\|` escapes). Only English keywords are supported. Doc strings carry no leading or trailing newline.

Verification: on the 187 feature files of the Forge repository (1,220 scenarios, 4,518 steps) the structure matches the `gherkin` crate's (names, tags, steps, doc strings, tables, examples) except for the surrounding newlines of doc strings.
