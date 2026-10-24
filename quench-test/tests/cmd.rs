//! Running programs and asserting on them (a POSIX shell stands in for the program under test).

#![allow(clippy::literal_string_with_formatting_args)]

use quench_test::cmd::{Command, Exactly};
use quench_test::predicate::prelude::*;
use std::time::Duration;

fn sh(script: &str) -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", script]);
    command
}

#[test]
fn success_failure_and_codes() {
    sh("echo out; echo err >&2")
        .assert()
        .success()
        .stdout(predicate::str::contains("out"))
        .stderr(predicate::str::contains("err"));
    sh("exit 3").assert().failure().code(3);
    sh("echo hi")
        .assert()
        .stdout("hi\n")
        .stdout(String::from("hi\n"))
        .stdout(Exactly(b"hi\n".to_vec()));
    sh("printf abc")
        .assert()
        .stdout(predicate::str::starts_with("a").and(predicate::str::ends_with("c")));
    let assert = sh("echo x").assert();
    assert!(assert.get_output().status.success());
    assert_eq!(assert.get_output().stdout, b"x\n");
}

#[test]
fn input_environment_and_directory() {
    sh("cat")
        .write_stdin("from stdin")
        .assert()
        .stdout("from stdin");
    sh("printf %s \"$ONE$TWO\"")
        .env("ONE", "1")
        .envs([("TWO", "2")])
        .assert()
        .stdout("12");
    sh("printf %s \"${GONE:-unset}\"")
        .env("GONE", "set")
        .env_remove("GONE")
        .assert()
        .stdout("unset");
    let dir = std::env::temp_dir();
    let expected = dir.canonicalize().unwrap();
    let out = sh("pwd -P").current_dir(&dir).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        expected.to_string_lossy()
    );
    // Large output on both pipes does not deadlock.
    sh("head -c 300000 /dev/zero; head -c 300000 /dev/zero >&2")
        .assert()
        .success();
    assert!(
        Command::new("sh")
            .arg("-c")
            .arg("exit 0")
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn timeouts_and_missing_programs() {
    let error = sh("sleep 5")
        .timeout(Duration::from_millis(100))
        .output()
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
    assert!(error.to_string().contains("did not finish"));
    assert!(Command::new("/definitely/not/here").output().is_err());
    assert!(
        Command::cargo_bin("no-such-binary-anywhere")
            .unwrap_err()
            .to_string()
            .contains("no-such-binary-anywhere")
    );
}

fn failure_text(check: impl FnOnce() + std::panic::UnwindSafe) -> String {
    let payload = std::panic::catch_unwind(check).unwrap_err();
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
        .unwrap_or_default()
}

#[test]
fn failed_assertions_show_everything() {
    let text = failure_text(|| {
        let _ = sh("echo seen; echo oops >&2; exit 2").assert().success();
    });
    assert!(
        text.contains("expected success")
            && text.contains("seen")
            && text.contains("oops")
            && text.contains("exit status: 2"),
        "{text}"
    );
    assert!(failure_text(|| drop(sh("true").assert().failure())).contains("expected failure"));
    assert!(failure_text(|| drop(sh("exit 1").assert().code(2))).contains("expected exit code 2"));
    let text = failure_text(|| {
        drop(
            sh("echo abc")
                .assert()
                .stdout(predicate::str::contains("zzz")),
        );
    });
    assert!(
        text.contains("expected stdout to be contains \"zzz\""),
        "{text}"
    );
    assert!(
        failure_text(|| drop(
            sh("echo abc")
                .assert()
                .stderr(predicate::str::is_empty().not())
        ))
        .contains("expected stderr")
    );
    assert!(
        failure_text(|| drop(sh("echo abc").assert().stdout("nope"))).contains("exactly \"nope\"")
    );
    assert!(
        failure_text(|| drop(Command::new("/definitely/not/here").assert()))
            .contains("could not run")
    );
}
