//! The result model, its JSON and the HTML page.

use quench_test::report::{self, Case, Report, Status, StepResult, Suite, file_stem, html};

fn sample() -> Report {
    let mut failing = Case::new("parser::rejects <bad> input", Status::Failed, 1500);
    failing.message = Some("assertion failed: a < b && \"c\"".to_string());
    failing.location = Some("tests/p.rs:12".to_string());
    let mut scenario = Case::new("Login works", Status::Passed, 70_000);
    scenario.tags = vec!["@smoke".to_string(), "@gatehouse".to_string()];
    scenario.steps = vec![
        StepResult {
            keyword: "Given".to_string(),
            text: "a user".to_string(),
            status: Status::Passed,
            duration_ms: 3,
            message: None,
        },
        StepResult {
            keyword: "Then".to_string(),
            text: "it fails".to_string(),
            status: Status::Failed,
            duration_ms: 4,
            message: Some("expected 200".to_string()),
        },
    ];
    let mut report = Report::new("demo & co");
    report.started = 1_700_000_000;
    report.duration_ms = 71_600;
    report.suites = vec![
        Suite {
            name: "unit".to_string(),
            kind: "unit".to_string(),
            cases: vec![
                Case::new("ok", Status::Passed, 5),
                failing,
                Case::new("later", Status::Skipped, 0),
            ],
        },
        Suite {
            name: "features/login".to_string(),
            kind: "bdd".to_string(),
            cases: vec![scenario],
        },
    ];
    report
}

#[test]
fn json_round_trips() {
    let report = sample();
    let text = report.to_json();
    assert!(
        text.contains("\"failed\": 1") && text.contains("\"total\": 4"),
        "{text}"
    );
    let back = Report::from_json(&text).unwrap();
    assert_eq!(back, report);
    assert_eq!(report.summary().total(), 4);
    assert!(report.failed());
    assert_eq!(report.suites[0].duration_ms(), 1505);
    assert!(Report::from_json("not json").is_none());
    assert!(Report::from_json("{\"title\": 1}").is_none());
    assert!(Report::from_json("{\"title\": \"t\", \"suites\": [{\"name\": 1}]}").is_none());
    let minimal = Report::from_json("{\"title\": \"t\", \"suites\": [{\"name\": \"s\", \"cases\": [{\"name\": \"c\", \"status\": \"weird\"}]}]}").unwrap();
    assert_eq!(
        (
            minimal.suites[0].kind.as_str(),
            minimal.suites[0].cases[0].status
        ),
        ("unit", Status::Skipped)
    );
}

#[test]
fn html_is_escaped_and_informative() {
    let page = html(&sample());
    assert!(
        page.starts_with("<!doctype html>")
            && page.contains("<title>demo &amp; co test report</title>"),
        "{page}"
    );
    assert!(
        page.contains("Some tests failed") && page.contains("2023-11-14 22:13:20 UTC"),
        "{page}"
    );
    assert!(
        page.contains("parser::rejects &lt;bad&gt; input")
            && page.contains("a &lt; b &amp;&amp; &quot;c&quot;")
    );
    assert!(!page.contains("<bad>"));
    assert!(page.contains("1m 10s") && page.contains("1.50s") && page.contains("5ms"));
    assert!(page.contains("@smoke") && page.contains("Given") && page.contains("expected 200"));
    assert!(page.contains("class=\"badge failed\"") && page.contains("class=\"badge skipped\""));
    // Suites with failures come first and are open.
    assert!(page.find("unit").unwrap() < page.find("features/login").unwrap());
    assert!(page.contains("<details open>"));
    assert!(page.contains("50.0%"), "pass rate");
    let green = Report::merge("all green", vec![Report::new("empty")]);
    let page = html(&green);
    assert!(page.contains("All tests passed") && page.contains("100.0%"));
}

#[test]
fn merging_adds_up() {
    let mut a = sample();
    a.started = 100;
    let mut b = sample();
    b.started = 50;
    let merged = Report::merge("both", vec![a, b]);
    assert_eq!(
        (
            merged.title.as_str(),
            merged.started,
            merged.duration_ms,
            merged.suites.len()
        ),
        ("both", 50, 143_200, 4)
    );
    assert_eq!(merged.summary().total(), 8);
}

#[test]
fn files_are_written_with_an_index() {
    let dir = std::env::temp_dir().join(format!("quench-test-report-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    report::write(&dir, "pkg one: a/b", &sample()).unwrap();
    let mut second = Report::new("second");
    second.suites = vec![Suite {
        name: "other".to_string(),
        kind: "unit".to_string(),
        cases: vec![Case::new("fine", Status::Passed, 1)],
    }];
    report::write(&dir, "second", &second).unwrap();
    assert_eq!(file_stem("pkg one: a/b"), "pkg-one--a-b");
    for name in [
        "pkg-one--a-b.json",
        "pkg-one--a-b.html",
        "second.json",
        "second.html",
        "index.html",
        "summary.json",
    ] {
        assert!(dir.join(name).is_file(), "{name}");
    }
    let summary =
        Report::from_json(&std::fs::read_to_string(dir.join("summary.json")).unwrap()).unwrap();
    assert_eq!(summary.suites.len(), 3);
    assert_eq!(summary.summary().total(), 5);
    // Stray files and unreadable reports in the directory are ignored.
    std::fs::write(dir.join("junk.json"), "{ nope").unwrap();
    std::fs::write(dir.join("notes.txt"), "x").unwrap();
    report::index(&dir).unwrap();
    assert_eq!(
        Report::from_json(&std::fs::read_to_string(dir.join("summary.json")).unwrap())
            .unwrap()
            .suites
            .len(),
        3
    );
    assert!(report::index(&dir.join("missing")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_directory_follows_the_environment() {
    // Environment variables are process-wide, so everything about them is in this one test.
    let saved: Vec<_> = ["QUENCH_TEST_REPORT", "QUENCH_TEST_REPORT_DIR"]
        .iter()
        .map(|k| (*k, std::env::var_os(k)))
        .collect();
    quench_core::env::remove("QUENCH_TEST_REPORT");
    quench_core::env::remove("QUENCH_TEST_REPORT_DIR");
    let default = report::report_directory().unwrap();
    assert!(default.ends_with("quench-test"), "{default:?}");
    quench_core::env::set("QUENCH_TEST_REPORT_DIR", "/tmp/somewhere");
    assert_eq!(
        report::report_directory().unwrap(),
        std::path::PathBuf::from("/tmp/somewhere")
    );
    quench_core::env::set("QUENCH_TEST_REPORT", "off");
    assert!(report::report_directory().is_none());
    for (key, value) in saved {
        match value {
            Some(v) => quench_core::env::set(key, v),
            None => quench_core::env::remove(key),
        }
    }
}
