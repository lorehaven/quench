//! Unit tests for `repl.rs`.
//!
//! The REPL reads the process's real standard input. The test runner's stdin may be anything (a closed
//! file, a terminal, a socket that never ends), so the EOF test re-runs this binary as a child with stdin
//! on `/dev/null`: `readline` then hits EOF at once, the same path a real Ctrl-D press takes. That is
//! enough to exercise `run`'s whole loop shape without needing a pty.

use quench_cli::repl::{ReplControl, run};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "QUENCH_CLI_REPL_EOF_CHILD";
const TEST_NAME: &str = "run_returns_ok_immediately_on_eof_stdin_without_calling_on_line";

#[quench_test::test]
fn run_returns_ok_immediately_on_eof_stdin_without_calling_on_line() {
    if std::env::var_os(CHILD_ENV).is_none() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg(TEST_NAME)
            .env(CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                panic!("the REPL did not stop at the end of its input");
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(status.success());
        return;
    }
    let mut calls = 0;
    let result = run("prompt> ", |_line| {
        calls += 1;
        ReplControl::Exit
    });

    assert!(result.is_ok());
    assert_eq!(calls, 0);
}

#[quench_test::test]
fn repl_control_variants_can_be_constructed() {
    assert!(matches!(
        ReplControl::Continue("next> ".to_string()),
        ReplControl::Continue(_)
    ));
    assert!(matches!(ReplControl::Exit, ReplControl::Exit));
}
