---
title: Configuration
parent: quench-test-bdd
grand_parent: Crates
nav_order: 3
---

## `Options`

```rust
#[derive(quench_args::Args, Debug, Clone, Default)]
pub struct Options {
    /// Run only scenarios whose tags satisfy this expression (`@fast and not @slow`)
    #[arg(long, short = 't', value_name = "EXPR")]
    pub tags: Option<String>,
    /// Run only scenarios whose name matches this regex
    #[arg(long, short = 'n', value_name = "REGEX")]
    pub name: Option<String>,
    /// How many scenarios run at once (default 64)
    #[arg(long, value_name = "N")]
    pub concurrency: Option<usize>,
    /// Skip the scenarios not yet started after the first failure
    #[arg(long)]
    pub fail_fast: bool,
    /// Write the JSON and HTML reports here instead of `target/<profile>/quench-test`
    #[arg(long, value_name = "DIR")]
    pub report_dir: Option<PathBuf>,
    /// Print only failures and the summary
    #[arg(long, short = 'q')]
    pub quiet: bool,
}
```

| Field | Flag | Meaning |
|---|---|---|
| `tags` | `-t`, `--tags EXPR` | tag expression (see [syntax](syntax.md#tag-expressions)); invalid: the run panics with `--tags: <message>` |
| `name` | `-n`, `--name REGEX` | the scenario name (after outline substitution) must match, unanchored; invalid: panic `--name: <error>` |
| `concurrency` | `--concurrency N` | scenarios at once; `0` is raised to 1; default 64 |
| `fail_fast` | `--fail-fast` | after the first failed scenario, scenarios that have not started are reported `skipped` (`skipped after an earlier failure`) |
| `report_dir` | `--report-dir DIR` | where to write the reports; otherwise `QUENCH_TEST_REPORT_DIR`/default as in [quench-test](../quench-test/configuration.md#environment-variables) |
| `quiet` | `-q`, `--quiet` | no `scenario ... ok` lines; failures and the summary are still printed |

Because it derives `Args`, `Options` is added to a command with `#[command(flatten)]` in a `quench-args` parser, and an `Options` value can also be built in code (all fields are public).

## Reports

The runner writes `<title>.json` and `<title>.html` (default title `bdd`, set with `Runner::title`) plus the merged `index.html`/`summary.json`, one suite per feature file (kind `bdd`) with a case per scenario (with tags like `@fast`, `file:line` location and per-step results). `QUENCH_TEST_REPORT=off` disables them. Unparsable files add a suite named after the file with one failed case `parse`.
