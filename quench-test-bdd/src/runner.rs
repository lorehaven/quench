//! Running feature files: scenario selection, concurrency, step matching, console output and reports.

use crate::expression::Matcher;
use crate::gherkin::{self, Feature, Rule, Scenario, Step};
use crate::options::Options;
use crate::step::{Context, Definition};
use crate::tags::Tags;
use crate::{STEPS, World};
use quench_core::future::catch_unwind;
use quench_regex::Regex;
use quench_test::report::{self, Case, Report, Status, StepResult, Suite};
use std::any::{Any, TypeId};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const DEFAULT_CONCURRENCY: usize = 64;

/// How many scenarios or steps ended each way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl Stats {
    #[must_use]
    pub const fn total(&self) -> usize {
        self.passed + self.failed + self.skipped
    }

    const fn add(&mut self, status: Status) {
        match status {
            Status::Passed => self.passed += 1,
            Status::Failed => self.failed += 1,
            Status::Skipped => self.skipped += 1,
        }
    }
}

/// The outcome of a run.
#[derive(Debug, Clone)]
pub struct Summary {
    pub features: usize,
    pub scenarios: Stats,
    pub steps: Stats,
    /// Feature files that could not be read or parsed.
    pub parse_errors: usize,
    pub report: Report,
}

impl Summary {
    /// Whether anything failed: a scenario, or a feature file that could not be parsed.
    #[must_use]
    pub const fn failed(&self) -> bool {
        self.scenarios.failed > 0 || self.parse_errors > 0
    }
}

type Filter = dyn Fn(&Feature, Option<&Rule>, &Scenario) -> bool + Send + Sync;

/// Runs the feature files for one [`World`] type.
pub struct Runner<W: World> {
    options: Options,
    filter: Option<Box<Filter>>,
    title: String,
    world: std::marker::PhantomData<fn() -> W>,
}

impl<W: World> Default for Runner<W> {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: World> std::fmt::Debug for Runner<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("title", &self.title)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

struct Job {
    feature: usize,
    name: String,
    tags: Vec<String>,
    location: String,
    steps: Vec<Step>,
    serial: bool,
}

struct Done {
    index: usize,
    status: Status,
    duration_ms: u64,
    message: Option<String>,
    steps: Vec<StepResult>,
}

fn feature_files(path: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .collect();
        entries.sort();
        for entry in entries {
            feature_files(&entry, out)?;
        }
    } else if path.extension().is_some_and(|e| e == "feature") {
        out.push(path.to_path_buf());
    }
    Ok(())
}

fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

impl<W: World> Runner<W> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            options: Options::default(),
            filter: None,
            title: "bdd".to_string(),
            world: std::marker::PhantomData,
        }
    }

    /// The command-line options (see [`Options`]).
    #[must_use]
    pub fn options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }

    /// An extra selection of scenarios, on top of the tag and name options.
    #[must_use]
    pub fn filter(
        mut self,
        filter: impl Fn(&Feature, Option<&Rule>, &Scenario) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.filter = Some(Box::new(filter));
        self
    }

    /// The report's title and file name (`bdd` by default).
    #[must_use]
    pub fn title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    fn jobs(&self, features: &[Feature]) -> Result<Vec<Job>, String> {
        let tags = self
            .options
            .tags
            .as_deref()
            .map(Tags::parse)
            .transpose()
            .map_err(|e| format!("--tags: {e}"))?;
        let name = self
            .options
            .name
            .as_deref()
            .map(Regex::new)
            .transpose()
            .map_err(|e| format!("--name: {e}"))?;
        let mut jobs = Vec::new();
        for (index, feature) in features.iter().enumerate() {
            let place = feature
                .path
                .as_ref()
                .map_or_else(String::new, |p| p.display().to_string());
            let mut add = |rule: Option<&Rule>, scenario: &Scenario| {
                let background = rule
                    .and_then(|r| r.background.as_ref())
                    .into_iter()
                    .chain(feature.background.as_ref());
                let mut steps: Vec<Step> = Vec::new();
                // Feature background first, then the rule's.
                let mut backgrounds: Vec<_> = background.collect();
                backgrounds.reverse();
                for b in backgrounds {
                    steps.extend(b.steps.iter().cloned());
                }
                for expanded in scenario.expand() {
                    let mut all_tags = feature.tags.clone();
                    if let Some(rule) = rule {
                        all_tags.extend(rule.tags.iter().cloned());
                    }
                    all_tags.extend(expanded.tags.iter().cloned());
                    if tags.as_ref().is_some_and(|t| !t.eval(&all_tags)) {
                        continue;
                    }
                    if name.as_ref().is_some_and(|n| !n.is_match(&expanded.name)) {
                        continue;
                    }
                    if self
                        .filter
                        .as_ref()
                        .is_some_and(|f| !f(feature, rule, &expanded))
                    {
                        continue;
                    }
                    let mut all_steps = steps.clone();
                    all_steps.extend(expanded.steps.iter().cloned());
                    jobs.push(Job {
                        feature: index,
                        name: expanded.name.clone(),
                        serial: all_tags.iter().any(|t| t == "serial"),
                        tags: all_tags,
                        location: format!("{place}:{}", expanded.line),
                        steps: all_steps,
                    });
                }
            };
            for scenario in &feature.scenarios {
                add(None, scenario);
            }
            for rule in &feature.rules {
                for scenario in &rule.scenarios {
                    add(Some(rule), scenario);
                }
            }
        }
        Ok(jobs)
    }

    /// Reads the feature files under `path` (a file, or a directory searched recursively), runs the selected
    /// scenarios, prints the results, writes the reports and returns the totals.
    ///
    /// # Panics
    ///
    /// On an invalid `--tags` expression or `--name` regex, or a step pattern that does not compile.
    pub async fn run(self, path: impl AsRef<Path>) -> Summary {
        let started = Instant::now();
        let mut report = Report::new(&self.title);
        let mut files = Vec::new();
        if let Err(e) = feature_files(path.as_ref(), &mut files) {
            eprintln!("cannot read {}: {e}", path.as_ref().display());
        }
        let mut features = Vec::new();
        let mut parse_errors = 0;
        let mut errors = Vec::new();
        for file in &files {
            match gherkin::parse_file(file) {
                Ok(feature) => features.push(feature),
                Err(e) => {
                    parse_errors += 1;
                    eprintln!("{e}");
                    errors.push((file.clone(), e.to_string()));
                }
            }
        }
        let jobs = match self.jobs(&features) {
            Ok(jobs) => jobs,
            Err(message) => panic!("{message}"),
        };

        let world_id = TypeId::of::<W>();
        let mut matchers: Vec<(Matcher, &'static Definition)> = Vec::new();
        for definition in &STEPS {
            if (definition.world)() == world_id {
                match Matcher::new(definition.pattern, definition.regex) {
                    Ok(matcher) => matchers.push((matcher, definition)),
                    Err(e) => panic!("{}:{}: {e}", definition.file, definition.line),
                }
            }
        }
        let matchers = Arc::new(matchers);

        quench_test::silence_panics(true);
        let results = self.execute(jobs.iter(), &matchers).await;
        quench_test::silence_panics(false);

        let mut summary = Summary {
            features: features.len(),
            scenarios: Stats::default(),
            steps: Stats::default(),
            parse_errors,
            report: Report::new(&self.title),
        };
        for feature in &features {
            report.suites.push(Suite {
                name: feature.name.clone(),
                kind: "bdd".to_string(),
                cases: Vec::new(),
            });
        }
        for (job, done) in jobs.iter().zip(&results) {
            summary.scenarios.add(done.status);
            for step in &done.steps {
                summary.steps.add(step.status);
            }
            let mut case = Case::new(&job.name, done.status, done.duration_ms);
            case.message.clone_from(&done.message);
            case.tags = job.tags.iter().map(|t| format!("@{t}")).collect();
            case.location = Some(job.location.clone());
            case.steps.clone_from(&done.steps);
            report.suites[job.feature].cases.push(case);
        }
        for (file, message) in errors {
            let mut case = Case::new("parse", Status::Failed, 0);
            case.message = Some(message);
            case.location = Some(file.display().to_string());
            report.suites.push(Suite {
                name: file.display().to_string(),
                kind: "bdd".to_string(),
                cases: vec![case],
            });
        }
        report.duration_ms = elapsed_ms(started);
        Self::print(&features, &jobs, &results, &summary, report.duration_ms);
        if let Some(dir) = self
            .options
            .report_dir
            .clone()
            .or_else(report::report_directory)
            && let Err(e) = report::write(&dir, &self.title, &report)
        {
            eprintln!("cannot write the report to {}: {e}", dir.display());
        }
        summary.report = report;
        summary
    }

    async fn execute<'a>(
        &self,
        jobs: impl Iterator<Item = &'a Job>,
        matchers: &Arc<Vec<(Matcher, &'static Definition)>>,
    ) -> Vec<Done> {
        let jobs: Vec<&Job> = jobs.collect();
        let limit = self
            .options
            .concurrency
            .unwrap_or(DEFAULT_CONCURRENCY)
            .max(1);
        let semaphore = Arc::new(Semaphore::new(limit));
        let failed = Arc::new(AtomicBool::new(false));
        let fail_fast = self.options.fail_fast;
        let mut results: Vec<Option<Done>> = (0..jobs.len()).map(|_| None).collect();

        let start = |index: usize, job: &Job, semaphore: Arc<Semaphore>| {
            let steps = job.steps.clone();
            let matchers = Arc::clone(matchers);
            let failed = Arc::clone(&failed);
            async move {
                let _permit = semaphore.acquire_owned().await.ok();
                if fail_fast && failed.load(Ordering::Relaxed) {
                    return Done {
                        index,
                        status: Status::Skipped,
                        duration_ms: 0,
                        message: Some("skipped after an earlier failure".to_string()),
                        steps: Vec::new(),
                    };
                }
                let done = scenario::<W>(index, &steps, &matchers).await;
                if done.status == Status::Failed {
                    failed.store(true, Ordering::Relaxed);
                }
                done
            }
        };

        let mut set = JoinSet::new();
        for (index, job) in jobs.iter().enumerate().filter(|(_, j)| !j.serial) {
            set.spawn(start(index, job, Arc::clone(&semaphore)));
        }
        while let Some(joined) = set.join_next().await {
            if let Ok(done) = joined {
                let index = done.index;
                self.progress(jobs[index], &done);
                results[index] = Some(done);
            }
        }
        for (index, job) in jobs.iter().enumerate().filter(|(_, j)| j.serial) {
            let done = start(index, job, Arc::clone(&semaphore)).await;
            self.progress(job, &done);
            results[index] = Some(done);
        }
        results
            .into_iter()
            .enumerate()
            .map(|(index, done)| {
                done.unwrap_or_else(|| Done {
                    index,
                    status: Status::Failed,
                    duration_ms: 0,
                    message: Some("the scenario task died".to_string()),
                    steps: Vec::new(),
                })
            })
            .collect()
    }

    fn progress(&self, job: &Job, done: &Done) {
        if self.options.quiet {
            return;
        }
        let mark = match done.status {
            Status::Passed => "ok",
            Status::Failed => "FAILED",
            Status::Skipped => "skipped",
        };
        println!("scenario {} ... {mark}", job.name);
    }

    fn print(
        features: &[Feature],
        jobs: &[Job],
        results: &[Done],
        summary: &Summary,
        duration_ms: u64,
    ) {
        let failures: Vec<_> = jobs
            .iter()
            .zip(results)
            .filter(|(_, d)| d.status == Status::Failed)
            .collect();
        if !failures.is_empty() {
            println!("\nfailures:");
            for (job, done) in failures {
                println!(
                    "\n  {} > {}  ({})",
                    features[job.feature].name, job.name, job.location
                );
                for step in done.steps.iter().filter(|s| s.status == Status::Failed) {
                    println!("    {} {}", step.keyword, step.text);
                    if let Some(message) = &step.message {
                        for line in message.lines() {
                            println!("      {line}");
                        }
                    }
                }
                if done.steps.is_empty()
                    && let Some(message) = &done.message
                {
                    println!("    {message}");
                }
            }
        }
        let s = &summary.scenarios;
        let t = &summary.steps;
        println!(
            "\nfeatures: {}; scenarios: {} ({} passed, {} failed, {} skipped); steps: {} ({} passed, {} failed, {} skipped); finished in {:.2}s",
            summary.features,
            s.total(),
            s.passed,
            s.failed,
            s.skipped,
            t.total(),
            t.passed,
            t.failed,
            t.skipped,
            Duration::from_millis(duration_ms).as_secs_f64()
        );
        if summary.parse_errors > 0 {
            println!(
                "{} feature file(s) could not be parsed",
                summary.parse_errors
            );
        }
    }
}

async fn guarded<F: Future>(future: F) -> Result<F::Output, String> {
    match catch_unwind(AssertUnwindSafe(future)).await {
        Ok(value) => Ok(value),
        Err(payload) => {
            let message = quench_test::panic_message(&*payload);
            Err(quench_test::take_panic_location().map_or_else(
                || format!("panicked: {message}"),
                |at| format!("panicked at {at}: {message}"),
            ))
        }
    }
}

async fn scenario<W: World>(
    index: usize,
    steps: &[Step],
    matchers: &[(Matcher, &'static Definition)],
) -> Done {
    let started = Instant::now();
    let mut results = Vec::with_capacity(steps.len());
    let mut message: Option<String> = None;
    let mut failed = false;
    let world = match guarded(W::new()).await {
        Ok(world) => Some(world),
        Err(e) => {
            failed = true;
            message = Some(format!("the world could not be created: {e}"));
            None
        }
    };
    let mut world = world;
    for step in steps {
        let begin = Instant::now();
        let mut result = StepResult {
            keyword: step.keyword.clone(),
            text: step.value.clone(),
            status: Status::Skipped,
            duration_ms: 0,
            message: None,
        };
        if !failed && let Some(world) = world.as_mut() {
            let outcome = run_step(world, step, matchers).await;
            match outcome {
                Ok(()) => result.status = Status::Passed,
                Err(e) => {
                    result.status = Status::Failed;
                    result.message = Some(e.clone());
                    message.get_or_insert(e);
                    failed = true;
                }
            }
        }
        result.duration_ms = elapsed_ms(begin);
        results.push(result);
    }
    Done {
        index,
        status: if failed {
            Status::Failed
        } else {
            Status::Passed
        },
        duration_ms: elapsed_ms(started),
        message,
        steps: results,
    }
}

async fn run_step<W: World>(
    world: &mut W,
    step: &Step,
    matchers: &[(Matcher, &'static Definition)],
) -> Result<(), String> {
    let mut found: Vec<(&'static Definition, Vec<String>)> = Vec::new();
    for (matcher, definition) in matchers {
        if definition.kind == step.kind
            && let Some(args) = matcher.matches(&step.value)
        {
            found.push((definition, args));
        }
    }
    let text = format!("{} {}", step.keyword, step.value);
    match found.len() {
        0 => Err(format!("undefined step: `{text}`")),
        1 => {
            let (definition, args) = found.remove(0);
            let context = Context { step, args };
            let any: &mut (dyn Any + Send) = world;
            guarded((definition.run)(any, &context)).await?
        }
        _ => {
            let places: Vec<String> = found
                .iter()
                .map(|(d, _)| format!("{}:{}", d.file, d.line))
                .collect();
            Err(format!(
                "ambiguous step `{text}` matches {}",
                places.join(", ")
            ))
        }
    }
}
