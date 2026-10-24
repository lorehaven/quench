//! Feature files: a parser for the Gherkin language (English keywords) and the outline expansion.
//!
//! Supported: `Feature`, `Rule`, `Background`, `Scenario`/`Example`, `Scenario Outline`/`Scenario Template`
//! with `Examples`/`Scenarios`, steps (`Given`, `When`, `Then`, `And`, `But`, `*`), tags, comments,
//! descriptions, doc strings (`"""` or ```` ``` ````) and data tables.

use std::fmt;
use std::path::{Path, PathBuf};

/// A feature file that cannot be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub file: Option<PathBuf>,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.file {
            Some(file) => write!(f, "{}:{}: {}", file.display(), self.line, self.message),
            None => write!(f, "line {}: {}", self.line, self.message),
        }
    }
}

impl std::error::Error for ParseError {}

/// Which of the three step roles a step plays (`And`, `But` and `*` take the role of the step before).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StepKind {
    Given,
    When,
    Then,
}

impl StepKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Given => "Given",
            Self::When => "When",
            Self::Then => "Then",
        }
    }
}

/// A table of cells, rows first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
}

/// One step of a scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The keyword as written (`Given`, `And`, `*`, ...).
    pub keyword: String,
    pub kind: StepKind,
    /// The text after the keyword.
    pub value: String,
    pub docstring: Option<String>,
    pub table: Option<Table>,
    pub line: usize,
}

impl Step {
    #[must_use]
    pub const fn docstring(&self) -> Option<&String> {
        self.docstring.as_ref()
    }

    #[must_use]
    pub const fn table(&self) -> Option<&Table> {
        self.table.as_ref()
    }
}

/// The `Examples` of a scenario outline. (Tags throughout are kept without their `@`, as in the `gherkin` crate.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Examples {
    pub name: String,
    pub tags: Vec<String>,
    /// The header row, then one row per example.
    pub table: Table,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub name: String,
    pub tags: Vec<String>,
    pub steps: Vec<Step>,
    /// Non-empty for a scenario outline.
    pub examples: Vec<Examples>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Background {
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub tags: Vec<String>,
    pub background: Option<Background>,
    pub scenarios: Vec<Scenario>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feature {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub background: Option<Background>,
    pub scenarios: Vec<Scenario>,
    pub rules: Vec<Rule>,
    pub path: Option<PathBuf>,
    pub line: usize,
}

struct Line<'a> {
    number: usize,
    indent: usize,
    text: &'a str,
}

fn lines(source: &str) -> Vec<Line<'_>> {
    source
        .lines()
        .enumerate()
        .map(|(i, raw)| {
            let trimmed = raw.trim_start();
            Line {
                number: i + 1,
                indent: raw.len() - trimmed.len(),
                text: trimmed.trim_end(),
            }
        })
        .collect()
}

fn keyword<'a>(text: &'a str, words: &[&str]) -> Option<&'a str> {
    words
        .iter()
        .find_map(|word| text.strip_prefix(word)?.strip_prefix(':').map(str::trim))
}

fn cells(text: &str) -> Vec<String> {
    let inner = text.trim().strip_prefix('|').unwrap_or(text);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('|') => cell.push('|'),
                Some('n') => cell.push('\n'),
                Some(other) => {
                    cell.push('\\');
                    cell.push(other);
                }
                None => cell.push('\\'),
            },
            '|' => out.push(std::mem::take(&mut cell).trim().to_string()),
            c => cell.push(c),
        }
    }
    out.push(cell.trim().to_string());
    out
}

struct Parser<'a> {
    lines: Vec<Line<'a>>,
    at: usize,
}

const STEP_WORDS: [(&str, Option<StepKind>); 6] = [
    ("Given ", Some(StepKind::Given)),
    ("When ", Some(StepKind::When)),
    ("Then ", Some(StepKind::Then)),
    ("And ", None),
    ("But ", None),
    ("* ", None),
];

impl<'a> Parser<'a> {
    fn error<T>(line: usize, message: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            file: None,
            line,
            message: message.into(),
        })
    }

    fn peek(&self) -> Option<&Line<'a>> {
        self.lines.get(self.at)
    }

    fn skip_blank(&mut self) {
        while self
            .peek()
            .is_some_and(|l| l.text.is_empty() || l.text.starts_with('#'))
        {
            self.at += 1;
        }
    }

    fn tags(&mut self) -> Vec<String> {
        let mut tags = Vec::new();
        loop {
            self.skip_blank();
            match self.peek() {
                Some(line) if line.text.starts_with('@') => {
                    tags.extend(
                        line.text
                            .split_whitespace()
                            .take_while(|w| !w.starts_with('#'))
                            .filter_map(|w| w.strip_prefix('@'))
                            .map(str::to_string),
                    );
                    self.at += 1;
                }
                _ => return tags,
            }
        }
    }

    fn is_structure(text: &str) -> bool {
        text.starts_with('@')
            || keyword(
                text,
                &[
                    "Scenario Outline",
                    "Scenario Template",
                    "Scenario",
                    "Example",
                    "Background",
                    "Rule",
                    "Examples",
                    "Scenarios",
                    "Feature",
                ],
            )
            .is_some()
    }

    /// Free text under a header, up to the first step or keyword.
    fn description(&mut self) -> String {
        let mut out: Vec<&str> = Vec::new();
        while let Some(line) = self.peek() {
            if line.text.starts_with('#') {
                self.at += 1;
                continue;
            }
            if Self::is_structure(line.text)
                || Self::step_start(line.text).is_some()
                || line.text.starts_with('|')
            {
                break;
            }
            out.push(line.text);
            self.at += 1;
        }
        out.join("\n").trim().to_string()
    }

    fn step_start(text: &str) -> Option<(&'static str, Option<StepKind>, &str)> {
        STEP_WORDS.iter().find_map(|(word, kind)| {
            text.strip_prefix(word)
                .map(|rest| (word.trim(), *kind, rest.trim()))
        })
    }

    fn steps(&mut self, previous: &mut StepKind) -> Result<Vec<Step>, ParseError> {
        let mut steps = Vec::new();
        loop {
            self.skip_blank();
            let Some(line) = self.peek() else {
                return Ok(steps);
            };
            let number = line.number;
            let Some((word, kind, value)) = Self::step_start(line.text) else {
                return Ok(steps);
            };
            let kind = kind.unwrap_or(*previous);
            *previous = kind;
            let value = value.to_string();
            self.at += 1;
            let mut step = Step {
                keyword: word.to_string(),
                kind,
                value,
                docstring: None,
                table: None,
                line: number,
            };
            loop {
                match self.peek() {
                    Some(next)
                        if next.text.starts_with("\"\"\"") || next.text.starts_with("```") =>
                    {
                        step.docstring = Some(self.docstring()?);
                    }
                    Some(next) if next.text.starts_with('|') => {
                        step.table = Some(self.table());
                    }
                    Some(next) if next.text.starts_with('#') => self.at += 1,
                    _ => break,
                }
            }
            steps.push(step);
        }
    }

    fn table(&mut self) -> Table {
        let mut rows = Vec::new();
        while let Some(line) = self.peek() {
            if line.text.starts_with('|') {
                rows.push(cells(line.text));
                self.at += 1;
            } else if line.text.starts_with('#') {
                self.at += 1;
            } else {
                break;
            }
        }
        Table { rows }
    }

    fn docstring(&mut self) -> Result<String, ParseError> {
        let open = self.lines[self.at].number;
        let indent = self.lines[self.at].indent;
        let fence = if self.lines[self.at].text.starts_with("\"\"\"") {
            "\"\"\""
        } else {
            "```"
        };
        self.at += 1;
        let mut body: Vec<String> = Vec::new();
        loop {
            let Some(line) = self.lines.get(self.at) else {
                return Self::error(open, "a doc string is never closed");
            };
            self.at += 1;
            if line.text == fence {
                return Ok(body.join("\n"));
            }
            // The raw line, minus the indentation of the opening fence.
            let raw = self.raw_line(line.number);
            let strip = raw
                .chars()
                .take(indent)
                .take_while(|c| *c == ' ' || *c == '\t')
                .count();
            body.push(raw[strip..].trim_end_matches('\r').to_string());
        }
    }

    fn raw_line(&self, number: usize) -> String {
        let line = &self.lines[number - 1];
        format!("{}{}", " ".repeat(line.indent), line.text)
    }

    fn background(&mut self) -> Result<Option<Background>, ParseError> {
        self.skip_blank();
        let Some(line) = self.peek() else {
            return Ok(None);
        };
        if keyword(line.text, &["Background"]).is_none() {
            return Ok(None);
        }
        self.at += 1;
        let _ = self.description();
        let mut previous = StepKind::Given;
        Ok(Some(Background {
            steps: self.steps(&mut previous)?,
        }))
    }

    fn scenario(&mut self, tags: Vec<String>) -> Result<Scenario, ParseError> {
        let line = self.lines[self.at].number;
        let text = self.lines[self.at].text;
        let (outline, name) =
            if let Some(name) = keyword(text, &["Scenario Outline", "Scenario Template"]) {
                (true, name)
            } else if let Some(name) = keyword(text, &["Scenario", "Example"]) {
                (false, name)
            } else {
                return Self::error(line, format!("expected a scenario, found `{text}`"));
            };
        let name = name.to_string();
        self.at += 1;
        let _ = self.description();
        let mut previous = StepKind::Given;
        let steps = self.steps(&mut previous)?;
        let mut examples = Vec::new();
        loop {
            let mark = self.at;
            let example_tags = self.tags();
            self.skip_blank();
            let Some(examples_name) = self
                .peek()
                .and_then(|l| keyword(l.text, &["Examples", "Scenarios"]))
            else {
                // Tags that belong to the next scenario: give them back.
                self.at = mark;
                break;
            };
            let examples_name = examples_name.to_string();
            self.at += 1;
            let _ = self.description();
            self.skip_blank();
            let table = if self.peek().is_some_and(|l| l.text.starts_with('|')) {
                self.table()
            } else {
                Table::default()
            };
            examples.push(Examples {
                name: examples_name,
                tags: example_tags,
                table,
            });
        }
        if outline && examples.is_empty() {
            return Self::error(line, "a scenario outline needs `Examples`");
        }
        if !outline && !examples.is_empty() {
            return Self::error(line, "`Examples` belong to a scenario outline");
        }
        Ok(Scenario {
            name,
            tags,
            steps,
            examples,
            line,
        })
    }

    fn feature(&mut self) -> Result<Feature, ParseError> {
        let tags = self.tags();
        self.skip_blank();
        let Some(line) = self.peek() else {
            return Self::error(1, "the file has no feature");
        };
        let number = line.number;
        let Some(name) = keyword(line.text, &["Feature"]) else {
            return Self::error(
                number,
                format!("expected `Feature:`, found `{}`", line.text),
            );
        };
        let name = name.to_string();
        self.at += 1;
        let description = self.description();
        let background = self.background()?;
        let mut feature = Feature {
            name,
            description,
            tags,
            background,
            scenarios: Vec::new(),
            rules: Vec::new(),
            path: None,
            line: number,
        };
        loop {
            let tags = self.tags();
            self.skip_blank();
            let Some(line) = self.peek() else { break };
            let number = line.number;
            if let Some(name) = keyword(line.text, &["Rule"]) {
                let name = name.to_string();
                self.at += 1;
                let _ = self.description();
                let background = self.background()?;
                let mut rule = Rule {
                    name,
                    tags,
                    background,
                    scenarios: Vec::new(),
                    line: number,
                };
                loop {
                    let mark = self.at;
                    let tags = self.tags();
                    self.skip_blank();
                    match self.peek() {
                        Some(next)
                            if keyword(
                                next.text,
                                &[
                                    "Scenario Outline",
                                    "Scenario Template",
                                    "Scenario",
                                    "Example",
                                ],
                            )
                            .is_some() =>
                        {
                            rule.scenarios.push(self.scenario(tags)?);
                        }
                        Some(next) if keyword(next.text, &["Rule"]).is_some() => {
                            self.at = mark;
                            break;
                        }
                        None => break,
                        Some(next) => {
                            return Self::error(next.number, format!("unexpected `{}`", next.text));
                        }
                    }
                }
                feature.rules.push(rule);
            } else if keyword(
                line.text,
                &[
                    "Scenario Outline",
                    "Scenario Template",
                    "Scenario",
                    "Example",
                ],
            )
            .is_some()
            {
                feature.scenarios.push(self.scenario(tags)?);
            } else {
                return Self::error(number, format!("unexpected `{}`", line.text));
            }
        }
        Ok(feature)
    }
}

/// Parses the text of a feature file.
///
/// # Errors
///
/// A [`ParseError`] naming the line of the first thing that is not Gherkin.
pub fn parse(source: &str) -> Result<Feature, ParseError> {
    let source = source.strip_prefix('\u{FEFF}').unwrap_or(source);
    let mut parser = Parser {
        lines: lines(source),
        at: 0,
    };
    parser.feature()
}

/// Reads and parses a feature file.
///
/// # Errors
///
/// A [`ParseError`] when the file cannot be read or is not Gherkin.
pub fn parse_file(path: &Path) -> Result<Feature, ParseError> {
    let text = std::fs::read_to_string(path).map_err(|e| ParseError {
        file: Some(path.to_path_buf()),
        line: 0,
        message: e.to_string(),
    })?;
    let mut feature = parse(&text).map_err(|mut e| {
        e.file = Some(path.to_path_buf());
        e
    })?;
    feature.path = Some(path.to_path_buf());
    Ok(feature)
}

fn substitute(text: &str, header: &[String], row: &[String]) -> String {
    let mut out = text.to_string();
    for (name, value) in header.iter().zip(row) {
        out = out.replace(&format!("<{name}>"), value);
    }
    out
}

impl Scenario {
    /// The scenarios an outline stands for, one per example row (a plain scenario is itself).
    #[must_use]
    pub fn expand(&self) -> Vec<Self> {
        if self.examples.is_empty() {
            return vec![self.clone()];
        }
        let mut out = Vec::new();
        for examples in &self.examples {
            let Some((header, rows)) = examples.table.rows.split_first() else {
                continue;
            };
            for row in rows {
                let mut scenario = self.clone();
                scenario.examples = Vec::new();
                scenario.name = substitute(&self.name, header, row);
                scenario.tags.extend(examples.tags.iter().cloned());
                for step in &mut scenario.steps {
                    step.value = substitute(&step.value, header, row);
                    step.docstring = step.docstring.as_ref().map(|d| substitute(d, header, row));
                    if let Some(table) = &mut step.table {
                        for cell_row in &mut table.rows {
                            for cell in cell_row {
                                *cell = substitute(cell, header, row);
                            }
                        }
                    }
                }
                out.push(scenario);
            }
        }
        out
    }
}
