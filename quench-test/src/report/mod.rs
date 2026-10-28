//! The result model shared by the unit-test runner and the Gherkin runner, and the files built from it: a
//! JSON document and a self-contained HTML page.
//!
//! Both runners write their results to the report directory (see [`report_directory`]) when they finish:
//! `<name>.json`, `<name>.html`, and an `index.html` / `summary.json` that merge every report in the
//! directory, so the latest `cargo test` leaves one page that covers all test binaries.

mod html;

use quench_serde::{Map, Value, json};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub use html::html;

/// How a test, scenario or step ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Passed,
    Failed,
    /// Not run: an ignored test, or a step after a failed one.
    Skipped,
}

impl Status {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "passed" => Self::Passed,
            "failed" => Self::Failed,
            _ => Self::Skipped,
        }
    }
}

/// One step of a scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    pub keyword: String,
    pub text: String,
    pub status: Status,
    pub duration_ms: u64,
    pub message: Option<String>,
}

/// One test, or one scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    pub status: Status,
    pub duration_ms: u64,
    /// The panic message or error of a failed case, or the reason it was skipped.
    pub message: Option<String>,
    pub tags: Vec<String>,
    /// Where the case is defined (`file:line`).
    pub location: Option<String>,
    pub steps: Vec<StepResult>,
}

impl Case {
    #[must_use]
    pub fn new(name: impl Into<String>, status: Status, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            status,
            duration_ms,
            message: None,
            tags: Vec::new(),
            location: None,
            steps: Vec::new(),
        }
    }
}

/// A test binary, or a feature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suite {
    pub name: String,
    /// `"unit"` or `"bdd"`.
    pub kind: String,
    pub cases: Vec<Case>,
}

/// How many cases ended each way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl Summary {
    #[must_use]
    pub const fn total(&self) -> usize {
        self.passed + self.failed + self.skipped
    }

    const fn add(&mut self, other: Self) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
    }

    fn to_value(self) -> Value {
        json!({ "total": self.total(), "passed": self.passed, "failed": self.failed, "skipped": self.skipped })
    }
}

impl Suite {
    #[must_use]
    pub fn summary(&self) -> Summary {
        let mut summary = Summary::default();
        for case in &self.cases {
            match case.status {
                Status::Passed => summary.passed += 1,
                Status::Failed => summary.failed += 1,
                Status::Skipped => summary.skipped += 1,
            }
        }
        summary
    }

    #[must_use]
    pub fn duration_ms(&self) -> u64 {
        self.cases.iter().map(|c| c.duration_ms).sum()
    }
}

/// The results of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub title: String,
    /// Seconds since the Unix epoch when the run started.
    pub started: u64,
    /// Wall-clock time of the run (cases may run concurrently, so this is not the sum of theirs).
    pub duration_ms: u64,
    pub suites: Vec<Suite>,
}

impl Report {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            started: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            duration_ms: 0,
            suites: Vec::new(),
        }
    }

    #[must_use]
    pub fn summary(&self) -> Summary {
        let mut total = Summary::default();
        for suite in &self.suites {
            total.add(suite.summary());
        }
        total
    }

    /// Whether any case failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.summary().failed > 0
    }

    /// Several reports as one: their suites together, the earliest start, the sum of the durations.
    #[must_use]
    pub fn merge(title: &str, reports: Vec<Self>) -> Self {
        let mut merged = Self::new(title);
        merged.started = reports
            .iter()
            .map(|r| r.started)
            .filter(|&s| s > 0)
            .min()
            .unwrap_or(merged.started);
        for report in reports {
            merged.duration_ms += report.duration_ms;
            merged.suites.extend(report.suites);
        }
        merged
    }

    /// The report as a JSON-ready value.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let suites: Vec<Value> = self.suites.iter().map(suite_value).collect();
        json!({
            "title": self.title,
            "started": self.started,
            "duration_ms": self.duration_ms,
            "summary": self.summary().to_value(),
            "suites": suites,
        })
    }

    /// The report as pretty-printed JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        quench_serde::json::to_string_pretty(&self.to_value())
    }

    /// Reads a report back from [`to_value`](Self::to_value)'s shape; `None` when it is not one.
    #[must_use]
    pub fn from_value(value: &Value) -> Option<Self> {
        let suites = value
            .get("suites")?
            .as_array()?
            .iter()
            .map(suite_from_value)
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            title: value.get("title")?.as_str()?.to_string(),
            started: value.get("started").and_then(Value::as_u64).unwrap_or(0),
            duration_ms: value
                .get("duration_ms")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            suites,
        })
    }

    /// Reads a report from JSON text.
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        Self::from_value(&quench_serde::json::from_str::<Value>(text).ok()?)
    }
}

fn step_value(step: &StepResult) -> Value {
    let mut map = Map::new();
    map.insert("keyword", Value::from(step.keyword.as_str()));
    map.insert("text", Value::from(step.text.as_str()));
    map.insert("status", Value::from(step.status.as_str()));
    map.insert("duration_ms", Value::from(step.duration_ms));
    if let Some(message) = &step.message {
        map.insert("message", Value::from(message.as_str()));
    }
    Value::Object(map)
}

fn case_value(case: &Case) -> Value {
    let mut map = Map::new();
    map.insert("name", Value::from(case.name.as_str()));
    map.insert("status", Value::from(case.status.as_str()));
    map.insert("duration_ms", Value::from(case.duration_ms));
    if let Some(message) = &case.message {
        map.insert("message", Value::from(message.as_str()));
    }
    if !case.tags.is_empty() {
        map.insert(
            "tags",
            Value::Array(case.tags.iter().map(|t| Value::from(t.as_str())).collect()),
        );
    }
    if let Some(location) = &case.location {
        map.insert("location", Value::from(location.as_str()));
    }
    if !case.steps.is_empty() {
        map.insert(
            "steps",
            Value::Array(case.steps.iter().map(step_value).collect()),
        );
    }
    Value::Object(map)
}

fn suite_value(suite: &Suite) -> Value {
    json!({
        "name": suite.name,
        "kind": suite.kind,
        "summary": suite.summary().to_value(),
        "duration_ms": suite.duration_ms(),
        "cases": suite.cases.iter().map(case_value).collect::<Vec<_>>(),
    })
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn step_from_value(value: &Value) -> Option<StepResult> {
    Some(StepResult {
        keyword: text(value, "keyword")?,
        text: text(value, "text")?,
        status: Status::parse(&text(value, "status")?),
        duration_ms: value
            .get("duration_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        message: text(value, "message"),
    })
}

fn case_from_value(value: &Value) -> Option<Case> {
    Some(Case {
        name: text(value, "name")?,
        status: Status::parse(&text(value, "status")?),
        duration_ms: value
            .get("duration_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        message: text(value, "message"),
        tags: value
            .get("tags")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        location: text(value, "location"),
        steps: value
            .get("steps")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(step_from_value).collect())
            .unwrap_or_default(),
    })
}

fn suite_from_value(value: &Value) -> Option<Suite> {
    Some(Suite {
        name: text(value, "name")?,
        kind: text(value, "kind").unwrap_or_else(|| "unit".to_string()),
        cases: value
            .get("cases")?
            .as_array()?
            .iter()
            .map(case_from_value)
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Where reports go.
///
/// `QUENCH_TEST_REPORT_DIR` if set, else `quench-test` next to the profile directory of the
/// running test binary (`target/debug/quench-test`), else `target/quench-test` under the current directory.
/// `None` when `QUENCH_TEST_REPORT` is `off` or `0`, and under `cargo nextest` (which sets `NEXTEST`) unless
/// `QUENCH_TEST_REPORT_DIR` asks for it: nextest runs every test in a process of its own, so no process sees the
/// whole run, and each would overwrite the report of the one before with its single test.
#[must_use]
pub fn report_directory() -> Option<PathBuf> {
    if matches!(
        quench_core::env::var("QUENCH_TEST_REPORT").as_deref(),
        Some("off" | "0" | "false")
    ) {
        return None;
    }
    if let Some(dir) = quench_core::env::var_os("QUENCH_TEST_REPORT_DIR").filter(|d| !d.is_empty())
    {
        return Some(PathBuf::from(dir));
    }
    if quench_core::env::var_os("NEXTEST").is_some() {
        return None;
    }
    let from_exe = std::env::current_exe().ok().and_then(|exe| {
        let deps = exe.parent()?;
        (deps.file_name()? == "deps")
            .then(|| deps.parent().map(|profile| profile.join("quench-test")))?
    });
    Some(from_exe.unwrap_or_else(|| PathBuf::from("target/quench-test")))
}

/// A file name made of letters, digits, `-` and `_`.
#[must_use]
pub fn file_stem(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// Writes `<stem>.json` and `<stem>.html` for `report` into `dir`, then rebuilds the merged `index.html` and
/// `summary.json` from every report there.
///
/// # Errors
///
/// When the directory or a file cannot be written.
pub fn write(dir: &Path, stem: &str, report: &Report) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let stem = file_stem(stem);
    write_atomic(
        &dir.join(format!("{stem}.json")),
        report.to_json().as_bytes(),
    )?;
    write_atomic(&dir.join(format!("{stem}.html")), html(report).as_bytes())?;
    index(dir)
}

/// Rebuilds `index.html` and `summary.json` from the other `.json` reports in `dir`.
///
/// # Errors
///
/// When the directory cannot be read or a file cannot be written.
pub fn index(dir: &Path) -> std::io::Result<()> {
    let mut reports = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|e| e == "json")
                && p.file_stem().is_some_and(|s| s != "summary")
        })
        .collect();
    entries.sort();
    for path in entries {
        if let Some(report) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| Report::from_json(&t))
        {
            reports.push(report);
        }
    }
    let merged = Report::merge("All test results", reports);
    write_atomic(&dir.join("summary.json"), merged.to_json().as_bytes())?;
    write_atomic(&dir.join("index.html"), html(&merged).as_bytes())
}

/// Writes through a temporary file so a reader (or a concurrent test binary) never sees half a file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temporary = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path)
}
