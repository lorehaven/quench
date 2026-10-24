//! The test runner: the standard harness's command line and output, run on a thread pool, plus the report
//! files.

use crate::case::{Outcome, ShouldPanic, TESTS, TestCase, capture_panics, install_hook, panicked};
use crate::report::{self, Case, Report, Status, Suite};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

/// The command-line options (the ones `cargo test` passes to the standard harness, and a few of ours).
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Run only tests whose name contains one of these (or equals, with `exact`).
    pub filters: Vec<String>,
    /// Leave out tests whose name contains one of these.
    pub skips: Vec<String>,
    pub exact: bool,
    /// Run only the ignored tests.
    pub only_ignored: bool,
    /// Run the ignored tests too.
    pub include_ignored: bool,
    /// Print the test names and stop.
    pub list: bool,
    pub threads: Option<usize>,
    /// One character per test.
    pub quiet: bool,
    /// Where to write the reports instead of the default directory.
    pub report_dir: Option<PathBuf>,
    pub help: bool,
}

const HELP: &str = "Usage: <test binary> [OPTIONS] [FILTER]...

Options:
      --exact                 Match test names exactly instead of by substring
      --skip <FILTER>         Skip tests whose name contains FILTER (repeatable)
      --ignored               Run only the ignored tests
      --include-ignored       Run the ignored tests as well
      --list                  List the tests and exit
      --test-threads <N>      Number of tests to run at once
  -q, --quiet                 One character per test
      --report-dir <DIR>      Write the JSON and HTML reports here
                              (default: target/<profile>/quench-test; QUENCH_TEST_REPORT=off disables them)
      --nocapture, --show-output, --color <WHEN>, --format <FMT>   Accepted and ignored
  -h, --help                  Print this help
";

impl Options {
    /// Reads the arguments after the program name. Unknown flags are ignored, as the standard harness's
    /// extra flags (`-Z ...`, `--logfile`) are of no use here.
    #[must_use]
    pub fn parse(args: &[String]) -> Self {
        let mut options = Self::default();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            let value =
                |iter: &mut std::slice::Iter<'_, String>| iter.next().cloned().unwrap_or_default();
            match arg.as_str() {
                "--exact" => options.exact = true,
                "--ignored" => options.only_ignored = true,
                "--include-ignored" => options.include_ignored = true,
                "--list" => options.list = true,
                "-q" | "--quiet" => options.quiet = true,
                "-h" | "--help" => options.help = true,
                "--skip" => options.skips.push(value(&mut iter)),
                "--test-threads" => {
                    options.threads = value(&mut iter).parse().ok().filter(|&n| n > 0);
                }
                "--report-dir" => options.report_dir = Some(PathBuf::from(value(&mut iter))),
                "--color" | "--format" | "--logfile" | "-Z" | "--report-time" | "--ensure-time" => {
                    if matches!(arg.as_str(), "--color" | "--format" | "--logfile" | "-Z") {
                        let _ = iter.next();
                    }
                }
                other if other.starts_with("--test-threads=") => {
                    options.threads = other["--test-threads=".len()..]
                        .parse()
                        .ok()
                        .filter(|&n| n > 0);
                }
                other if other.starts_with("--skip=") => {
                    options.skips.push(other["--skip=".len()..].to_string());
                }
                other if other.starts_with("--report-dir=") => {
                    options.report_dir = Some(PathBuf::from(&other["--report-dir=".len()..]));
                }
                other if other.starts_with('-') => {}
                other => options.filters.push(other.to_string()),
            }
        }
        options
    }

    fn selects(&self, name: &str) -> bool {
        let named = |filter: &String| {
            if self.exact {
                name == filter
            } else {
                name.contains(filter.as_str())
            }
        };
        (self.filters.is_empty() || self.filters.iter().any(named))
            && !self.skips.iter().any(|skip| {
                if self.exact {
                    name == skip
                } else {
                    name.contains(skip.as_str())
                }
            })
    }
}

/// What a run came to.
#[derive(Debug)]
pub struct RunResult {
    /// 0 when nothing failed, 101 otherwise (as the standard harness exits).
    pub exit_code: i32,
    pub report: Report,
}

struct Finished {
    status: Status,
    message: Option<String>,
    duration: Duration,
}

fn execute(case: &TestCase) -> Finished {
    let start = Instant::now();
    capture_panics(case.name);
    let outcome =
        std::panic::catch_unwind(case.run).unwrap_or_else(|payload| panicked(payload.as_ref()));
    let (status, message) = match (outcome, case.should_panic) {
        (Outcome::Passed, ShouldPanic::No) | (Outcome::Panicked(_), ShouldPanic::Yes) => {
            (Status::Passed, None)
        }
        (Outcome::Passed, _) => (
            Status::Failed,
            Some("test did not panic as expected".to_string()),
        ),
        (Outcome::Panicked(message), ShouldPanic::WithMessage(expected)) => {
            if message.contains(expected) {
                (Status::Passed, None)
            } else {
                (
                    Status::Failed,
                    Some(format!(
                        "panic did not contain expected string\n      panic message: {message:?}\n expected substring: {expected:?}"
                    )),
                )
            }
        }
        (Outcome::Panicked(message) | Outcome::Failed(message), _) => {
            (Status::Failed, Some(message))
        }
    };
    Finished {
        status,
        message,
        duration: start.elapsed(),
    }
}

/// Runs `cases` selected by `options`, printing in the standard harness's format to `out`.
pub fn run(
    cases: &[&'static TestCase],
    options: &Options,
    out: &mut (dyn Write + Send),
) -> RunResult {
    let mut selected: Vec<&'static TestCase> = cases
        .iter()
        .copied()
        .filter(|c| options.selects(c.name))
        .collect();
    selected.sort_by_key(|c| c.name);
    let filtered_out = cases.len() - selected.len();
    let title = report_title();

    if options.list {
        for case in &selected {
            let _ = writeln!(out, "{}: test", case.name);
        }
        let _ = writeln!(out, "\n{} tests, 0 benchmarks", selected.len());
        return RunResult {
            exit_code: 0,
            report: Report::new(title),
        };
    }

    let runs = |case: &TestCase| {
        if options.only_ignored {
            case.ignore.is_some()
        } else {
            case.ignore.is_none() || options.include_ignored
        }
    };
    let skipped_count = selected
        .iter()
        .filter(|c| !runs(c) && !options.only_ignored)
        .count();
    let to_run: Vec<&'static TestCase> = selected.iter().copied().filter(|c| runs(c)).collect();
    let not_selected_by_ignore = if options.only_ignored {
        selected.len() - to_run.len()
    } else {
        0
    };

    let _ = writeln!(
        out,
        "\nrunning {} test{}",
        to_run.len() + skipped_count,
        if to_run.len() + skipped_count == 1 {
            ""
        } else {
            "s"
        }
    );
    let started = Instant::now();
    let mut report = Report::new(title.clone());
    install_hook();

    let results: Vec<Mutex<Option<Finished>>> = to_run.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let printer = Mutex::new(Printer {
        out,
        quiet: options.quiet,
        column: 0,
    });
    // Ignored tests are reported first, in name order, as the standard harness does.
    for case in selected
        .iter()
        .filter(|c| !runs(c) && !options.only_ignored)
    {
        printer.lock().unwrap_or_else(PoisonError::into_inner).line(
            case.name,
            "ignored",
            'i',
            case.ignore,
        );
    }
    let threads = options
        .threads
        .or_else(|| {
            std::thread::available_parallelism()
                .ok()
                .map(std::num::NonZero::get)
        })
        .unwrap_or(1)
        .clamp(1, to_run.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(case) = to_run.get(index) else { break };
                    let finished = execute(case);
                    let (word, glyph) = match finished.status {
                        Status::Passed => ("ok", '.'),
                        _ => ("FAILED", 'F'),
                    };
                    printer
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .line(case.name, word, glyph, None);
                    *results[index]
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(finished);
                }
            });
        }
    });
    let mut printer = printer.into_inner().unwrap_or_else(PoisonError::into_inner);
    printer.end_line();

    let mut passed = 0;
    let mut failures: Vec<(&str, String)> = Vec::new();
    let mut suite = Suite {
        name: title,
        kind: "unit".to_string(),
        cases: Vec::new(),
    };
    for case in selected
        .iter()
        .filter(|c| !runs(c) && !options.only_ignored)
    {
        let mut entry = Case::new(case.name, Status::Skipped, 0);
        entry.message = Some(match case.ignore {
            Some(reason) if !reason.is_empty() => format!("ignored: {reason}"),
            _ => "ignored".to_string(),
        });
        entry.location = Some(format!("{}:{}", case.file, case.line));
        suite.cases.push(entry);
    }
    for (case, result) in to_run.iter().zip(&results) {
        let finished = result
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("every test ran");
        if finished.status == Status::Passed {
            passed += 1;
        } else {
            failures.push((case.name, finished.message.clone().unwrap_or_default()));
        }
        let mut entry = Case::new(
            case.name,
            finished.status,
            u64::try_from(finished.duration.as_millis()).unwrap_or(u64::MAX),
        );
        entry.message = finished.message;
        entry.location = Some(format!("{}:{}", case.file, case.line));
        suite.cases.push(entry);
    }
    report.suites.push(suite);
    report.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let out = printer.out;
    if !failures.is_empty() {
        let _ = writeln!(out, "\nfailures:\n");
        for (name, message) in &failures {
            let _ = writeln!(out, "---- {name} stdout ----\n{message}\n");
        }
        let _ = writeln!(out, "\nfailures:");
        for (name, _) in &failures {
            let _ = writeln!(out, "    {name}");
        }
    }
    let failed = failures.len();
    let _ = writeln!(
        out,
        "\ntest result: {}. {passed} passed; {failed} failed; {skipped_count} ignored; 0 measured; {} filtered out; finished in {:.2}s\n",
        if failed == 0 { "ok" } else { "FAILED" },
        filtered_out + not_selected_by_ignore,
        started.elapsed().as_secs_f64()
    );
    RunResult {
        exit_code: i32::from(failed > 0) * 101,
        report,
    }
}

struct Printer<'a> {
    out: &'a mut (dyn Write + Send),
    quiet: bool,
    column: usize,
}

impl Printer<'_> {
    fn line(&mut self, name: &str, word: &str, glyph: char, reason: Option<&str>) {
        if self.quiet {
            let _ = write!(self.out, "{glyph}");
            self.column += 1;
            if self.column == 100 {
                let _ = writeln!(self.out);
                self.column = 0;
            }
        } else {
            match reason.filter(|r| !r.is_empty()) {
                Some(reason) => {
                    let _ = writeln!(self.out, "test {name} ... {word}, {reason}");
                }
                None => {
                    let _ = writeln!(self.out, "test {name} ... {word}");
                }
            }
        }
        let _ = self.out.flush();
    }

    fn end_line(&mut self) {
        if self.quiet && self.column > 0 {
            let _ = writeln!(self.out);
        }
    }
}

/// `<package> <binary>`, from Cargo's environment and the running executable.
fn report_title() -> String {
    let package = std::env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "tests".to_string());
    let binary = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .map_or_else(|| "tests".to_string(), |stem| strip_hash(&stem));
    format!("{package} · {binary}")
}

/// `unit-3f2a9c01d4e5b6a7` → `unit`.
fn strip_hash(stem: &str) -> String {
    match stem.rsplit_once('-') {
        Some((name, hash)) if hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) => {
            name.to_string()
        }
        _ => stem.to_string(),
    }
}

/// Runs every registered test with the process's arguments, writes the reports, and returns the exit code.
#[must_use]
pub fn run_registered() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = Options::parse(&args);
    if options.help {
        print!("{HELP}");
        return 0;
    }
    let cases: Vec<&'static TestCase> = TESTS.iter().collect();
    let mut stdout = std::io::stdout();
    let result = run(&cases, &options, &mut stdout);
    if !options.list
        && let Some(dir) = options.report_dir.clone().or_else(report::report_directory)
    {
        let stem = result.report.title.replace(" · ", "-");
        if let Err(e) = report::write(&dir, &stem, &result.report) {
            eprintln!(
                "warning: could not write the test report to {}: {e}",
                dir.display()
            );
        }
    }
    result.exit_code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_follow_the_standard_command_line() {
        let args: Vec<String> = [
            "a::b",
            "--exact",
            "--skip",
            "slow",
            "--skip=flaky",
            "--test-threads=3",
            "-q",
            "--nocapture",
            "--color",
            "never",
            "--include-ignored",
            "--ignored",
            "--list",
            "--report-dir",
            "out",
            "-h",
            "--unknown",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let o = Options::parse(&args);
        assert_eq!(o.filters, ["a::b"]);
        assert_eq!(o.skips, ["slow", "flaky"]);
        assert!(o.exact && o.quiet && o.include_ignored && o.only_ignored && o.list && o.help);
        assert_eq!(o.threads, Some(3));
        assert_eq!(o.report_dir, Some(PathBuf::from("out")));
        assert!(o.selects("a::b") && !o.selects("a::bc") && !o.selects("slow"));
        let o = Options::parse(&[
            "--test-threads".to_string(),
            "0".to_string(),
            "--report-dir=x".to_string(),
        ]);
        assert_eq!((o.threads, o.report_dir), (None, Some(PathBuf::from("x"))));
        let loose = Options::parse(&["b".to_string()]);
        assert!(loose.selects("a::b") && loose.selects("b::c") && !loose.selects("a::c"));
    }

    #[test]
    fn test_binary_names_lose_their_hash() {
        assert_eq!(strip_hash("unit-0123456789abcdef"), "unit");
        assert_eq!(strip_hash("unit"), "unit");
        assert_eq!(
            strip_hash("my-test-0123456789abcdeg"),
            "my-test-0123456789abcdeg"
        );
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;

    fn passes() -> Outcome {
        Outcome::Passed
    }
    fn fails() -> Outcome {
        Outcome::Failed("Error: \"nope\"".to_string())
    }
    fn panics() -> Outcome {
        Outcome::Panicked("thread 'x' panicked at f.rs:1:1:\nboom happened".to_string())
    }
    fn really_panics() -> Outcome {
        panic!("escaped the wrapper");
    }

    static PASS: TestCase = case("m::pass", passes, None, ShouldPanic::No);
    static FAIL: TestCase = case("m::fail", fails, None, ShouldPanic::No);
    static PANIC: TestCase = case("m::panic", panics, None, ShouldPanic::No);
    static EXPECTED_PANIC: TestCase = case("m::expected_panic", panics, None, ShouldPanic::Yes);
    static EXPECTED_TEXT: TestCase = case(
        "m::expected_text",
        panics,
        None,
        ShouldPanic::WithMessage("boom"),
    );
    static WRONG_TEXT: TestCase = case(
        "m::wrong_text",
        panics,
        None,
        ShouldPanic::WithMessage("other"),
    );
    static NO_PANIC: TestCase = case("m::no_panic", passes, None, ShouldPanic::Yes);
    static ERR_BUT_SHOULD_PANIC: TestCase =
        case("m::err_should_panic", fails, None, ShouldPanic::Yes);
    static IGNORED: TestCase = case("m::ignored", passes, Some("slow"), ShouldPanic::No);
    static BARE_IGNORED: TestCase = case("m::bare_ignored", fails, Some(""), ShouldPanic::No);
    static ESCAPES: TestCase = case("other::escapes", really_panics, None, ShouldPanic::No);

    const fn case(
        name: &'static str,
        run: fn() -> Outcome,
        ignore: Option<&'static str>,
        should_panic: ShouldPanic,
    ) -> TestCase {
        TestCase {
            name,
            file: "tests/x.rs",
            line: 7,
            ignore,
            should_panic,
            run,
        }
    }

    fn all() -> Vec<&'static TestCase> {
        vec![
            &PASS,
            &FAIL,
            &PANIC,
            &EXPECTED_PANIC,
            &EXPECTED_TEXT,
            &WRONG_TEXT,
            &NO_PANIC,
            &ERR_BUT_SHOULD_PANIC,
            &IGNORED,
            &BARE_IGNORED,
            &ESCAPES,
        ]
    }

    fn go(cases: &[&'static TestCase], args: &[&str]) -> (RunResult, String) {
        let options = Options::parse(&args.iter().map(ToString::to_string).collect::<Vec<_>>());
        let mut out: Vec<u8> = Vec::new();
        let result = run(cases, &options, &mut out);
        (result, String::from_utf8(out).unwrap())
    }

    #[test]
    fn a_run_prints_like_the_standard_harness() {
        let (result, text) = go(&all(), &["--test-threads=2"]);
        assert_eq!(result.exit_code, 101);
        assert!(text.contains("running 11 tests"), "{text}");
        assert!(text.contains("test m::pass ... ok"));
        assert!(text.contains("test m::fail ... FAILED"));
        assert!(text.contains("test m::ignored ... ignored, slow"));
        assert!(text.contains("test m::bare_ignored ... ignored\n"));
        assert!(text.contains("---- m::fail stdout ----\nError: \"nope\""));
        assert!(text.contains(
            "---- m::panic stdout ----\nthread 'x' panicked at f.rs:1:1:\nboom happened"
        ));
        assert!(
            text.contains("---- m::wrong_text stdout ----\npanic did not contain expected string")
        );
        assert!(text.contains("---- m::no_panic stdout ----\ntest did not panic as expected"));
        assert!(
            text.contains("---- other::escapes stdout ----\nthread 'other::escapes' panicked at ")
                && text.contains("escaped the wrapper"),
            "{text}"
        );
        assert!(
            text.contains("\nfailures:\n    m::err_should_panic\n"),
            "{text}"
        );
        assert!(text.contains("test result: FAILED. 3 passed; 6 failed; 2 ignored; 0 measured; 0 filtered out; finished in "), "{text}");
        let suite = &result.report.suites[0];
        assert_eq!(suite.kind, "unit");
        assert_eq!(suite.cases.len(), 11);
        let summary = result.report.summary();
        assert_eq!((summary.passed, summary.failed, summary.skipped), (3, 6, 2));
        let ignored = suite.cases.iter().find(|c| c.name == "m::ignored").unwrap();
        assert_eq!(
            (ignored.status, ignored.message.as_deref()),
            (Status::Skipped, Some("ignored: slow"))
        );
        assert_eq!(ignored.location.as_deref(), Some("tests/x.rs:7"));
    }

    #[test]
    fn a_passing_run_exits_zero() {
        let (result, text) = go(&[&PASS, &EXPECTED_PANIC, &EXPECTED_TEXT, &IGNORED], &[]);
        assert_eq!(result.exit_code, 0);
        assert!(
            text.contains("test result: ok. 3 passed; 0 failed; 1 ignored;"),
            "{text}"
        );
        assert!(!text.contains("failures:"));
    }

    #[test]
    fn filters_select_and_count_the_rest() {
        let (_, text) = go(&all(), &["m::pass"]);
        assert!(
            text.contains("running 1 test\n") && text.contains("10 filtered out"),
            "{text}"
        );
        let (_, text) = go(&all(), &["pass", "--skip", "m::pass", "--exact"]);
        assert!(text.contains("running 0 tests"), "{text}");
        let (_, text) = go(&all(), &["m::pass", "--exact"]);
        assert!(text.contains("running 1 test\n"), "{text}");
    }

    #[test]
    fn ignored_tests_run_on_request() {
        let (result, text) = go(&all(), &["--ignored"]);
        assert!(
            text.contains("running 2 tests")
                && text.contains("test m::ignored ... ok")
                && text.contains("test m::bare_ignored ... FAILED"),
            "{text}"
        );
        assert_eq!(result.exit_code, 101);
        let (_, text) = go(&[&IGNORED, &PASS], &["--include-ignored"]);
        assert!(
            text.contains("running 2 tests")
                && text.contains("test m::ignored ... ok")
                && text.contains("2 passed; 0 failed; 0 ignored"),
            "{text}"
        );
    }

    #[test]
    fn listing_and_quiet_mode() {
        let (result, text) = go(&[&PASS, &IGNORED], &["--list"]);
        assert_eq!(
            text,
            "m::ignored: test\nm::pass: test\n\n2 tests, 0 benchmarks\n"
        );
        assert_eq!(result.exit_code, 0);
        let (_, text) = go(&all(), &["-q", "--test-threads", "1"]);
        let dots = text
            .lines()
            .find(|l| l.chars().all(|c| matches!(c, '.' | 'F' | 'i')) && !l.is_empty())
            .unwrap();
        assert_eq!(dots.len(), 11, "{text}");
        let many: Vec<&'static TestCase> = std::iter::repeat_n(&PASS, 120).collect();
        let (_, text) = go(&many, &["-q"]);
        assert!(text.lines().any(|l| l.len() == 100), "{text}");
    }

    #[test]
    fn help_is_available() {
        assert!(HELP.contains("--test-threads"));
        assert!(Options::parse(&["--help".to_string()]).help);
    }
}
