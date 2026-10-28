//! The command line as a person types it: parsing, exit codes and what gets run.

use super::support::*;
use quench_tool::execute;
use quench_tool::watch::{Event, Watch};
use std::path::Path;

struct Script(Vec<Event>);

impl Watch for Script {
    fn poll(&mut self) -> Event {
        if self.0.is_empty() {
            Event::Stop
        } else {
            self.0.remove(0)
        }
    }
}

fn go(args: &[&str], sys: &Fake, cwd: &Path) -> (i32, String) {
    let mut out = Vec::new();
    let code = execute(args.iter().copied(), sys, &mut out, cwd, &mut |_| {
        Box::new(Script(vec![]))
    });
    (code, text(out))
}

#[quench_test::test]
fn help_lists_every_command() {
    let (code, out) = go(&["--help"], &Fake::default(), &tmp("help"));
    assert_eq!(code, 0);
    for word in ["new", "dev", "build", "doctor", "test"] {
        assert!(out.contains(word), "{word} missing from:\n{out}");
    }
}

#[quench_test::test]
fn version_is_printed() {
    let (code, out) = go(&["--version"], &Fake::default(), &tmp("version"));
    assert_eq!(code, 0);
    assert!(out.contains("quench"), "{out}");
}

#[quench_test::test]
fn a_mistake_in_the_arguments_is_exit_code_two() {
    assert_eq!(go(&["frobnicate"], &Fake::default(), &tmp("bad")).0, 2);
    assert_eq!(go(&["new"], &Fake::default(), &tmp("noname")).0, 2);
    assert_eq!(
        go(
            &["new", "x", "--template", "mobile"],
            &Fake::default(),
            &tmp("tpl")
        )
        .0,
        2
    );
}

#[quench_test::test]
fn new_makes_the_project() {
    let dir = tmp("new");
    let (code, out) = go(
        &[
            "new",
            "demo",
            "--template",
            "web",
            "--quench-path",
            env!("CARGO_MANIFEST_DIR"),
        ],
        &Fake::default(),
        &dir,
    );
    assert_eq!(code, 0, "{out}");
    assert!(dir.join("demo/quench.toml").is_file());
    assert!(out.contains("quench dev"), "{out}");
    let (again, _) = go(&["new", "demo"], &Fake::default(), &dir);
    assert_eq!(again, 1, "a second `new` must not overwrite the first");
}

#[quench_test::test]
fn commands_that_need_a_project_say_where_to_look() {
    let dir = tmp("outside");
    for cmd in ["dev", "build", "test"] {
        let (code, _) = go(&[cmd], &Fake::default(), &dir);
        assert_eq!(code, 1, "{cmd}");
    }
}

#[quench_test::test]
fn test_runs_cargo_test_and_then_the_browser_build_check() {
    let p = project("test-cmd", "web");
    let sys = Fake::default();
    let (code, _) = go(&["test", "my_filter"], &sys, &p.root);
    assert_eq!(code, 0);
    assert_eq!(
        sys.lines(),
        [
            "cargo test --workspace my_filter",
            "cargo check --target wasm32-unknown-unknown -p demo-app",
        ]
    );
}

#[quench_test::test]
fn a_failing_test_run_stops_before_the_check() {
    let p = project("test-fail", "web");
    let sys = Fake::default();
    *sys.fail_on.borrow_mut() = Some("cargo test".to_string());
    let (code, _) = go(&["test"], &sys, &p.root);
    assert_eq!(code, 1);
    assert_eq!(sys.lines().len(), 1);
}

#[quench_test::test]
fn doctor_passes_or_fails_by_what_web_development_needs() {
    let (code, out) = go(&["doctor"], &Fake::default(), &tmp("doctor"));
    assert_eq!(code, 1);
    assert!(out.contains("MISSING"), "{out}");
}

#[quench_test::test]
fn build_and_dev_run_from_inside_the_project() {
    let p = project("inside", "web");
    let sys = Fake::default();
    let nested = p.root.join("app/src");
    std::fs::create_dir_all(&nested).unwrap();
    let (code, _) = go(&["build"], &sys, &nested);
    assert_eq!(code, 0);
    assert_eq!(sys.lines().len(), 2);
    let (code, out) = go(&["dev", "--port", "4000"], &sys, &nested);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("http://127.0.0.1:4000"), "{out}");
}

#[quench_test::test]
fn the_flags_of_dev_and_test_are_accepted() {
    let p = project("flags", "web");
    let sys = Fake::default();
    assert_eq!(go(&["dev", "--open"], &sys, &p.root).0, 0);
    let (code, out) = go(&["test", "--browser"], &sys, &p.root);
    assert_eq!(code, 1, "the project has no tests/browser.mjs here: {out}");
}

#[quench_test::test]
fn android_init_and_build_bundle_are_commands() {
    let p = project("android-cli", "web");
    let sys = Fake::default();
    let (code, out) = go(
        &["android", "init", "--package", "dev.lorehaven.demo"],
        &sys,
        &p.root,
    );
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("dev.lorehaven.demo"), "{out}");
    assert_eq!(
        go(&["android", "init"], &sys, &p.root).0,
        1,
        "it exists now"
    );
    assert_eq!(
        go(&["android"], &sys, &p.root).0,
        2,
        "a subcommand is needed"
    );
    let (code, _) = go(&["build", "--bundle"], &sys, &p.root);
    assert_eq!(code, 0);
    assert!(sys.lines().iter().any(|l| l.ends_with("debug/demo-server")));
    assert_eq!(go(&["android", "init"], &sys, &tmp("android-none")).0, 1);
}
