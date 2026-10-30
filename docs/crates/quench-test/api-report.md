---
title: Reports
parent: quench-test
grand_parent: Crates
nav_order: 5
---

Module `report`: the result model shared by the unit-test runner and the Gherkin runner ([quench-test-bdd](../quench-test-bdd/index.md)), and the JSON and HTML files built from it. A finished run writes `<name>.json`, `<name>.html`, and an `index.html` / `summary.json` that merge every report in the directory, so the latest `cargo test` leaves one page for all test binaries. The HTML is a single file with no external resources (inline style and script), light and dark (`prefers-color-scheme`), with a text filter and All/Failed/Passed/Skipped buttons; it reads fine without the script.

## Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status { Passed, Failed, Skipped }    // Skipped: an ignored test, or a step after a failed one
impl Status { #[must_use] pub const fn as_str(self) -> &'static str }   // "passed" | "failed" | "skipped"

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult { pub keyword: String, pub text: String, pub status: Status, pub duration_ms: u64, pub message: Option<String> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub name: String, pub status: Status, pub duration_ms: u64,
    pub message: Option<String>,        // panic message / error, or the reason it was skipped
    pub tags: Vec<String>,
    pub location: Option<String>,       // "file:line"
    pub steps: Vec<StepResult>,
}
impl Case { #[must_use] pub fn new(name: impl Into<String>, status: Status, duration_ms: u64) -> Self }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suite { pub name: String, pub kind: String /* "unit" or "bdd" */, pub cases: Vec<Case> }
impl Suite { #[must_use] pub fn summary(&self) -> Summary;  #[must_use] pub fn duration_ms(&self) -> u64 /* sum of the cases */ }

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary { pub passed: usize, pub failed: usize, pub skipped: usize }
impl Summary { #[must_use] pub const fn total(&self) -> usize }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report { pub title: String, pub started: u64 /* Unix seconds */, pub duration_ms: u64 /* wall clock */, pub suites: Vec<Suite> }
impl Report {
    #[must_use] pub fn new(title: impl Into<String>) -> Self                  // started = now
    #[must_use] pub fn summary(&self) -> Summary
    #[must_use] pub fn failed(&self) -> bool
    #[must_use] pub fn merge(title: &str, reports: Vec<Self>) -> Self         // suites together, earliest non-zero start, summed durations
    #[must_use] pub fn to_value(&self) -> Value
    #[must_use] pub fn to_json(&self) -> String                               // pretty-printed
    #[must_use] pub fn from_value(value: &Value) -> Option<Self>
    #[must_use] pub fn from_json(text: &str) -> Option<Self>
}
```

## Functions

```rust
pub use html::html;                       // pub fn html(report: &Report) -> String
#[must_use] pub fn report_directory() -> Option<PathBuf>
#[must_use] pub fn file_stem(name: &str) -> String
pub fn write(dir: &Path, stem: &str, report: &Report) -> std::io::Result<()>
pub fn index(dir: &Path) -> std::io::Result<()>
```

| Function | Behaviour |
|---|---|
| `html(report)` | a complete HTML document titled `<title> test report` |
| `report_directory()` | see [configuration](configuration.md#environment-variables): `None` when reports are off |
| `file_stem(name)` | keeps ASCII letters, digits, `-`, `_`; every other character becomes `-` |
| `write(dir, stem, report)` | creates `dir`, writes `<stem>.json` and `<stem>.html` (via a temporary file and rename, so readers never see half a file), then calls `index` |
| `index(dir)` | merges every `*.json` in `dir` other than `summary.json` that parses as a report (sorted by file name) into `summary.json` and `index.html` with the title `All test results` |

## JSON shape

```json
{
  "title": "quench-args · unit",
  "started": 1793374787,
  "duration_ms": 58,
  "summary": { "total": 6, "passed": 5, "failed": 0, "skipped": 1 },
  "suites": [
    {
      "name": "quench-args · unit", "kind": "unit",
      "summary": { "total": 6, "passed": 5, "failed": 0, "skipped": 1 },
      "duration_ms": 58,
      "cases": [
        { "name": "unit::slow", "status": "skipped", "duration_ms": 0, "message": "ignored: slow", "location": "tests/unit.rs:17" },
        { "name": "unit::adds", "status": "passed", "duration_ms": 0, "location": "tests/unit.rs:6" }
      ]
    }
  ]
}
```

`message`, `tags`, `location` and `steps` are omitted when empty. A BDD case adds `tags` (with `@`) and `steps`, each `{ "keyword", "text", "status", "duration_ms", "message"? }`. The unit runner makes **one suite per test binary** (named like the report title), a BDD run makes one suite per feature file (kind `bdd`) plus one per unparsable file with a failed case named `parse`.
