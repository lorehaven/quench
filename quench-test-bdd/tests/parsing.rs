use quench_test_bdd::expression::Matcher;
use quench_test_bdd::gherkin::{self, StepKind};
use quench_test_bdd::tags::Tags;

fn args(pattern: &str, text: &str) -> Option<Vec<String>> {
    Matcher::new(pattern, false).expect("pattern").matches(text)
}

#[test]
fn expressions_capture_typed_parameters() {
    assert_eq!(
        args("I have {int} cucumber(s)", "I have 12 cucumbers"),
        Some(vec!["12".into()])
    );
    assert_eq!(
        args("I have {int} cucumber(s)", "I have -1 cucumber"),
        Some(vec!["-1".into()])
    );
    assert_eq!(args("I have {int} cucumber(s)", "I have x cucumber"), None);
    assert_eq!(
        args("pi is {float}", "pi is 3.14"),
        Some(vec!["3.14".into()])
    );
    assert_eq!(
        args("pi is {float}", "pi is .5e3"),
        Some(vec![".5e3".into()])
    );
    assert_eq!(
        args("a {word} here", "a red here"),
        Some(vec!["red".into()])
    );
    assert_eq!(
        args("anything {}", "anything at all"),
        Some(vec!["at all".into()])
    );
    assert_eq!(
        args("say {string} to {string}", r#"say "hi \"you\"" to 'me'"#),
        Some(vec![r#"hi "you""#.into(), "me".into()])
    );
    assert_eq!(args("a/an apple", "an apple"), Some(vec![]));
    assert_eq!(args("a/an apple", "the apple"), None);
    assert_eq!(
        args(r"cost \(usd\) 5\/6 \{x}", "cost (usd) 5/6 {x}"),
        Some(vec![])
    );
    assert_eq!(args("a.b", "axb"), None);
    assert_eq!(args(r"back\\slash", r"back\slash"), Some(vec![]));
}

#[test]
fn bad_patterns_are_rejected() {
    for pattern in ["{nope}", "open {int", "open (maybe"] {
        assert!(Matcher::new(pattern, false).is_err(), "{pattern}");
    }
    assert!(Matcher::new("(", true).is_err());
    let regex = Matcher::new(r"^(\d+) and (x)?$", true).expect("regex");
    assert_eq!(
        regex.matches("5 and "),
        Some(vec!["5".into(), String::new()])
    );
    assert_eq!(regex.matches("5 and x"), Some(vec!["5".into(), "x".into()]));
}

#[test]
fn tag_expressions() {
    let tags = |t: &[&str]| t.iter().map(ToString::to_string).collect::<Vec<_>>();
    let e = Tags::parse("@a and (@b or not @c)").expect("parse");
    assert!(e.eval(&tags(&["a", "b"])));
    assert!(e.eval(&tags(&["a"])));
    assert!(!e.eval(&tags(&["a", "c"])));
    assert!(!e.eval(&tags(&["b"])));
    assert!(
        Tags::parse("not not @x")
            .expect("parse")
            .eval(&tags(&["x"]))
    );
    for bad in ["", "@a and", "(@a", "@a @b", "and @a", "@a )", ")"] {
        assert!(Tags::parse(bad).is_err(), "{bad:?}");
    }
}

const FEATURE: &str = "\u{feff}# a comment
@top @second
Feature: Shop
  Free text
  # comment
  more text

  Background:
    Given a shop

  @fast
  Scenario: one
    # inside
    Given a thing
      | a | b\\|c |
      | 1 | 2 |
    And another
    * star
    But not this
    When it happens
      ```
      fenced
      ```
    Then done

  @x
  Scenario Outline: out <v>
    Given <v> with
      \"\"\"
        deep <v>
      \"\"\"

    @e1
    Examples: first
      | v |
      | a |
    Examples:
      | v |
      | b |

  Rule: R
    Background:
      When rule setup
    @late
    Example: two
      Then ok
";

#[test]
fn gherkin_structure() {
    let f = gherkin::parse(FEATURE).expect("parse");
    assert_eq!(f.name, "Shop");
    assert_eq!(f.tags, ["top", "second"]);
    assert_eq!(f.description, "Free text\nmore text");
    assert_eq!(f.background.as_ref().expect("background").steps.len(), 1);
    assert_eq!(f.scenarios.len(), 2);
    let one = &f.scenarios[0];
    assert_eq!(one.tags, ["fast"]);
    let kinds: Vec<_> = one
        .steps
        .iter()
        .map(|s| (s.keyword.as_str(), s.kind))
        .collect();
    assert_eq!(kinds[1], ("And", StepKind::Given));
    assert_eq!(kinds[2], ("*", StepKind::Given));
    assert_eq!(kinds[3], ("But", StepKind::Given));
    assert_eq!(kinds[4].1, StepKind::When);
    assert_eq!(kinds[5].1, StepKind::Then);
    assert_eq!(
        one.steps[0].table().expect("table").rows,
        [["a", "b|c"], ["1", "2"]]
    );
    assert_eq!(one.steps[4].docstring().expect("doc"), "fenced");
    let expanded = f.scenarios[1].expand();
    assert_eq!(expanded.len(), 2);
    assert_eq!(expanded[0].name, "out a");
    assert_eq!(expanded[0].tags, ["x", "e1"]);
    assert_eq!(expanded[1].tags, ["x"]);
    assert_eq!(expanded[0].steps[0].docstring().expect("doc"), "  deep a");
    assert_eq!(f.rules.len(), 1);
    assert_eq!(f.rules[0].scenarios[0].tags, ["late"]);
    assert_eq!(
        f.rules[0]
            .background
            .as_ref()
            .expect("rule background")
            .steps[0]
            .kind,
        StepKind::When
    );
    assert_eq!(one.expand().len(), 1);
}

#[test]
fn gherkin_errors_name_the_line() {
    for (text, line) in [
        ("", 1),
        ("Scenario: x\n", 1),
        (
            "Feature: f\n  Scenario: s\n    Given a\n      \"\"\"\n      open\n",
            4,
        ),
        ("Feature: f\n  Scenario Outline: s\n    Given a\n", 2),
        (
            "Feature: f\n  Scenario: s\n    Given a\n    Examples:\n      | a |\n",
            2,
        ),
        ("Feature: f\n  Rule: r\n    Given nonsense\n", 3),
        ("Feature: f\n  Given nonsense\n", 2),
    ] {
        let error = gherkin::parse(text).expect_err(text);
        assert_eq!(error.line, line, "{text:?}: {error}");
        assert!(error.to_string().contains(&format!("line {line}")));
    }
    let error =
        gherkin::parse_file(std::path::Path::new("/nonexistent/x.feature")).expect_err("missing");
    assert!(error.to_string().contains("x.feature"));
}

#[test]
fn step_arguments_and_outcomes() {
    use quench_test_bdd::step::{Context, StepOutcome, parse_argument};
    let feature = gherkin::parse("Feature: f\n  Scenario: s\n    Given x\n").expect("parse");
    let context = Context {
        step: &feature.scenarios[0].steps[0],
        args: vec!["7".into(), "nope".into()],
    };
    assert_eq!(parse_argument::<u8>(&context, 0, "n"), Ok(7));
    assert!(
        parse_argument::<u8>(&context, 1, "n")
            .expect_err("not a number")
            .contains("`nope` as `n`")
    );
    assert!(
        parse_argument::<u8>(&context, 2, "n")
            .expect_err("missing")
            .contains("no argument")
    );
    assert!(format!("{context:?}").contains("Context"));
    assert_eq!(().into_result(), Ok(()));
    assert_eq!(Err::<(), _>("bad").into_result(), Err("bad".to_string()));
}

#[test]
fn options_come_from_the_command_line() {
    use quench_args::{Args, Command};
    use quench_test_bdd::Options;
    let command = Options::augment_args(Command::new("t"));
    let matches = command
        .try_get_matches_from([
            "-t",
            "@a",
            "--name",
            "x",
            "--concurrency",
            "3",
            "--fail-fast",
            "-q",
            "--report-dir",
            "out",
        ])
        .expect("parse");
    let options = <Options as quench_args::FromMatches>::from_matches(&matches).expect("options");
    assert_eq!(options.tags.as_deref(), Some("@a"));
    assert_eq!(options.name.as_deref(), Some("x"));
    assert_eq!(options.concurrency, Some(3));
    assert!(options.fail_fast && options.quiet);
    assert_eq!(options.report_dir, Some("out".into()));
}
