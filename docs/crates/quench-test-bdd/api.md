---
title: API reference
parent: quench-test-bdd
grand_parent: Crates
nav_order: 2
---

Public items of `quench_test_bdd`, in the order of `src/lib.rs`. No `unsafe`.

## Crate root

```rust
pub mod expression;
pub mod gherkin;
pub mod step;
pub mod tags;
pub use gherkin::{Feature, Rule, Scenario, Step, Table};
pub use options::Options;                 // see configuration
pub use quench_test_bdd_macros::{World, given, then, when};
pub use runner::{Runner, Stats, Summary};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub static STEPS: quench_core::registry::Registry<step::Definition>;

pub trait World: Sized + Send + 'static {
    fn new() -> BoxFuture<'static, Self>;
}
```

`STEPS` is the registry the step attributes add to. `World` is the state of one scenario, created fresh for each; normally derived (`#[derive(World)]`, with `#[world(init = path)]` for an `async fn() -> Self`, else `Default`). `World` and the three step attributes share names with macros; `use quench_test_bdd::World;` brings the derive and the trait.

## `struct Runner<W: World>`

`Debug` shows title and options. `Default` is `new()`.

```rust
impl<W: World> Runner<W> {
    #[must_use] pub fn new() -> Self
    #[must_use] pub fn options(self, options: Options) -> Self
    #[must_use] pub fn filter(self, filter: impl Fn(&Feature, Option<&Rule>, &Scenario) -> bool + Send + Sync + 'static) -> Self
    #[must_use] pub fn title(self, title: &str) -> Self
    pub async fn run(self, path: impl AsRef<Path>) -> Summary
}
```

| Method | Behaviour |
|---|---|
| `new()` | default options, no filter, title `bdd` |
| `options(o)` | the command-line options |
| `filter(f)` | an extra selection on top of `--tags` and `--name`: `f(feature, rule, scenario)` is called for each (expanded) scenario; false skips it |
| `title(t)` | the report title and file name |
| `run(path)` | reads the `.feature` files under `path` (a file, or a directory searched recursively, entries sorted), runs the selected scenarios, prints, writes the reports and returns the totals. **Panics** on an invalid `--tags`/`--name` or a step pattern that does not compile. Must be awaited inside a Tokio runtime (it spawns tasks). |

Running order: all scenarios not tagged `serial` are spawned concurrently (limited by a semaphore of `concurrency`), results are collected as they finish; then the `serial` ones run one by one. For a scenario: the world is built (`World::new`; a panic there is reported `the world could not be created: ..`), each step runs in order (backgrounds first), after the first failure the remaining steps are `skipped`. Step panics become failures with `panicked at <file:line:col>: <message>`.

## `struct Summary` and `struct Stats`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats { pub passed: usize, pub failed: usize, pub skipped: usize }
impl Stats { #[must_use] pub const fn total(&self) -> usize }

#[derive(Debug, Clone)]
pub struct Summary {
    pub features: usize,        // feature files parsed
    pub scenarios: Stats,
    pub steps: Stats,
    pub parse_errors: usize,    // feature files that could not be read or parsed
    pub report: quench_test::report::Report,
}
impl Summary { #[must_use] pub const fn failed(&self) -> bool }   // scenarios.failed > 0 || parse_errors > 0
```

## Module `step`

```rust
pub use crate::gherkin::StepKind as Kind;

#[derive(Debug)]
pub struct Context<'a> { pub step: &'a Step, pub args: Vec<String> }

pub struct Definition {
    pub kind: Kind,
    pub pattern: &'static str,
    pub regex: bool,                    // pattern is a regex, not a cucumber expression
    pub file: &'static str,
    pub line: u32,
    pub world: fn() -> TypeId,
    pub run: for<'a> fn(&'a mut (dyn Any + Send), &'a Context<'a>) -> BoxFuture<'a, Result<(), String>>,
}                                       // Debug prints kind, pattern, regex, "at file:line"

pub fn parse_argument<T>(context: &Context<'_>, index: usize, label: &str) -> Result<T, String>
    where T: FromStr, T::Err: Display

pub trait StepOutcome { fn into_result(self) -> Result<(), String>; }
impl StepOutcome for ()
impl<E: Display> StepOutcome for Result<(), E>
```

`Context.args` are the captures in order. `parse_argument` reads capture `index` as a `T`; errors: ``the pattern captures no argument for `label` `` and ``cannot read `raw` as `label`: <error>``. These items exist for the macros; you only need `Context.step` if you write a `Definition` by hand.

## Module `gherkin`

```rust
pub fn parse(source: &str) -> Result<Feature, ParseError>
pub fn parse_file(path: &Path) -> Result<Feature, ParseError>      // sets Feature::path

pub struct ParseError { pub file: Option<PathBuf>, pub line: usize, pub message: String }   // Debug, Clone, PartialEq, Eq, Display, Error
pub enum StepKind { Given, When, Then }                  // Debug, Clone, Copy, PartialEq, Eq, Hash;  const fn name(self) -> &'static str
pub struct Table { pub rows: Vec<Vec<String>> }          // Default
pub struct Step { pub keyword: String, pub kind: StepKind, pub value: String,
                  pub docstring: Option<String>, pub table: Option<Table>, pub line: usize }
impl Step { pub const fn docstring(&self) -> Option<&String>; pub const fn table(&self) -> Option<&Table> }
pub struct Examples { pub name: String, pub tags: Vec<String>, pub table: Table }   // header row first
pub struct Scenario { pub name: String, pub tags: Vec<String>, pub steps: Vec<Step>, pub examples: Vec<Examples>, pub line: usize }
impl Scenario { #[must_use] pub fn expand(&self) -> Vec<Self> }
pub struct Background { pub steps: Vec<Step> }
pub struct Rule { pub name: String, pub tags: Vec<String>, pub background: Option<Background>, pub scenarios: Vec<Scenario>, pub line: usize }
pub struct Feature { pub name: String, pub description: String, pub tags: Vec<String>, pub background: Option<Background>,
                     pub scenarios: Vec<Scenario>, pub rules: Vec<Rule>, pub path: Option<PathBuf>, pub line: usize }
```

All derive `Debug, Clone, PartialEq, Eq` (`Background` also `Default`). Tags are stored without `@`. `Scenario::expand` returns one scenario per example row (a plain scenario is itself), with `<column>` placeholders substituted in name, step values, doc strings and table cells and the Examples tags appended. `parse` strips a BOM; `parse_file` errors carry the path (`line` is 0 for an unreadable file).

## Module `expression`

```rust
#[derive(Debug, Clone)]
pub struct Matcher { /* compiled regex */ }
impl Matcher {
    pub fn new(pattern: &str, regex: bool) -> Result<Self, String>
    #[must_use] pub fn matches(&self, text: &str) -> Option<Vec<String>>
}
```

`new` compiles a cucumber expression (or a regex when `regex` is true). `matches` returns the captured arguments in order if the text matches (the whole text for expressions; anywhere for regexes). `{string}` arguments are returned unquoted and unescaped.

## Module `tags`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tags { Tag(String), Not(Box<Tags>), And(Box<Tags>, Box<Tags>), Or(Box<Tags>, Box<Tags>) }
impl Tags {
    pub fn parse(text: &str) -> Result<Self, String>
    #[must_use] pub fn eval(&self, tags: &[String]) -> bool     // tags with or without '@'
}
```
