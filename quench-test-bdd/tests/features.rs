// Step functions take their captures by value, as the attribute parses them.
#![allow(
    clippy::needless_pass_by_value,
    clippy::missing_const_for_fn,
    clippy::similar_names,
    clippy::unused_async,
    clippy::needless_pass_by_ref_mut
)]

use quench_test_bdd::{Options, Runner, Step, World, given, then, when};
use std::path::{Path, PathBuf};

#[derive(World, Default)]
struct Calc {
    total: i64,
    words: Vec<String>,
}

#[given("a total of {int}")]
fn start(world: &mut Calc, n: i64) {
    world.total = n;
}

#[when("I add {int}")]
fn add(world: &mut Calc, n: i64) {
    world.total += n;
}

#[when(regex = r"^I multiply by (\d+)$")]
async fn multiply(world: &mut Calc, n: i64) {
    world.total *= n;
}

#[when("I say {string}")]
fn say(world: &mut Calc, word: String) {
    world.words.push(word);
}

#[when("I read the doc string")]
fn doc(world: &mut Calc, step: &Step) {
    world
        .words
        .push(step.docstring().expect("a doc string").clone());
}

#[when("I read the table")]
fn table(world: &mut Calc, step: &Step) {
    let rows = &step.table().expect("a table").rows;
    world.total = i64::try_from(rows.len()).expect("small");
}

#[then("the total is {int}")]
fn check(world: &mut Calc, n: i64) -> Result<(), String> {
    if world.total == n {
        Ok(())
    } else {
        Err(format!("total is {}", world.total))
    }
}

#[then("the words are {string}")]
fn words(world: &mut Calc, joined: String) {
    assert_eq!(world.words.join(","), joined);
}

#[then("a cucumber is {word}")]
fn cucumber(_: &mut Calc) {}

#[then("the last word has {int} lines")]
fn lines(world: &mut Calc, n: usize) {
    assert_eq!(world.words.last().map_or(0, |w| w.lines().count()), n);
}

#[then("it blows up")]
fn blow(_: &mut Calc) {
    panic!("boom");
}

#[then("a cucumber/cucumbers is/are ripe")]
fn ripe(_: &mut Calc) {}

fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("quench-bdd-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("dir");
    for (file, text) in files {
        std::fs::write(root.join(file), text).expect("write");
    }
    root
}

fn run(root: &Path, options: Options) -> quench_test_bdd::Summary {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("rt");
    runtime.block_on(Runner::<Calc>::new().options(options).title("t").run(root))
}

fn options(root: &Path) -> Options {
    Options {
        report_dir: Some(root.join("out")),
        quiet: true,
        ..Options::default()
    }
}

const GOOD: &str = r#"
@math
Feature: Arithmetic
  Some words
  about it.

  Background:
    Given a total of 1

  Scenario: add
    When I add 2
    Then the total is 3

  @serial
  Scenario Outline: outline <n>
    When I add <n>
    And I multiply by 2
    Then the total is <result>

    Examples:
      | n | result |
      | 1 | 4      |
      | 2 | 6      |

  Rule: strings
    Scenario: say
      When I say "a \"quoted\" word"
      And I say 'b'
      Then the words are "a \"quoted\" word,b"

    Scenario: docs and tables
      When I read the doc string
        """
        hello
          indented
        """
      Then the last word has 2 lines
      When I read the table
        | a | b |
        | 1 | 2 |
      Then the total is 2
      Then a cucumbers are ripe
"#;

#[test]
fn passing_features_are_reported() {
    let root = dir("good", &[("a.feature", GOOD)]);
    let summary = run(&root, options(&root));
    assert!(!summary.failed(), "{:?}", summary.report.to_json());
    assert_eq!(summary.scenarios.passed, 5);
    assert!(summary.steps.total() > 10);
    let json = std::fs::read_to_string(root.join("out/t.json")).expect("json");
    assert!(json.contains("Arithmetic") && json.contains("outline 2"));
    assert!(root.join("out/t.html").exists() && root.join("out/index.html").exists());
}

#[test]
fn failures_undefined_and_ambiguous_steps_fail_their_scenario() {
    let text = "Feature: Bad\n  Scenario: wrong\n    Given a total of 1\n    Then the total is 2\n    And the total is 1\n\n  Scenario: undefined\n    Given nothing matches this\n\n  Scenario: panics\n    Then it blows up\n\n  Scenario: ambiguous\n    Then a cucumber is ripe\n";
    let root = dir("bad", &[("b.feature", text)]);
    let summary = run(&root, options(&root));
    assert_eq!(summary.scenarios.failed, 4);
    assert_eq!(summary.scenarios.passed, 0);
    let json = summary.report.to_json();
    assert!(
        json.contains("total is 1") && json.contains("undefined step") && json.contains("boom"),
        "{json}"
    );
    assert_eq!(summary.steps.skipped, 1);
}

#[test]
fn tags_names_and_filters_select_scenarios() {
    let root = dir("sel", &[("a.feature", GOOD)]);
    let mut o = options(&root);
    o.tags = Some("@math and not @serial".to_string());
    assert_eq!(run(&root, o).scenarios.total(), 3);
    let mut o = options(&root);
    o.name = Some("^outline".to_string());
    assert_eq!(run(&root, o).scenarios.total(), 2);
}

#[test]
fn fail_fast_skips_the_rest() {
    let text = "Feature: F\n  Scenario: one\n    Then it blows up\n\n  Scenario: two\n    Given a total of 1\n";
    let root = dir("ff", &[("c.feature", text)]);
    let mut o = options(&root);
    o.fail_fast = true;
    o.concurrency = Some(1);
    let summary = run(&root, o);
    assert_eq!(
        (summary.scenarios.failed, summary.scenarios.skipped),
        (1, 1)
    );
}

#[test]
fn unparsable_files_fail_the_run() {
    let root = dir("parse", &[("d.feature", "Scenario: orphan\n")]);
    let summary = run(&root, options(&root));
    assert!(summary.failed());
    assert_eq!(summary.parse_errors, 1);
}
