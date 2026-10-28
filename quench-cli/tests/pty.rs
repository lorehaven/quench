//! The terminal code on a real (pseudo) terminal.
//!
//! The editor and the terminal queries work on the process's own descriptors, so the test re-runs its own
//! binary as a child under `script` (which gives it a pty for stdin and stdout), types into `script`,
//! and reads the outcome back from a file. Needs `script` and `stty` on the path.
#![cfg(unix)]
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_cli::editor::LineEditor;
use quench_cli::term::{is_tty, read_secret, size_of};
use quench_cli::tty::RawMode;
use std::fs;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const CHILD_ENV: &str = "QUENCH_CLI_PTY_OUTCOME_FILE";

/// Runs only inside the child: reads one line and records the outcome.
#[quench_test::test]
fn child_reads_one_line() {
    let Ok(path) = std::env::var(CHILD_ENV) else {
        return;
    };
    let mut editor = LineEditor::new();
    editor.add_history_entry("from history");
    let first = editor.readline("\x1b[1mchild\x1b[0m> ").unwrap();
    let second = editor.readline("again> ").unwrap();
    fs::write(path, format!("{first:?}\n{second:?}\n")).unwrap();
}

/// The terminal's attributes, as `stty` takes them back.
fn attributes() -> String {
    let out = Command::new("stty")
        .arg("-g")
        .stdin(Stdio::inherit())
        .output()
        .unwrap();
    let said = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(!said.is_empty(), "stty -g said nothing");
    said
}

/// Runs only inside the child: asks about the terminal, switches it and reads a secret.
#[quench_test::test]
fn child_asks_about_the_terminal() {
    let Ok(path) = std::env::var(CHILD_ENV) else {
        return;
    };
    let mut report = Vec::<String>::new();
    report.push(format!("tty {} {} {}\n", is_tty(0), is_tty(1), is_tty(-1)));
    assert!(
        Command::new("stty")
            .args(["rows", "30", "cols", "100"])
            .status()
            .unwrap()
            .success()
    );
    report.push(format!("size {:?}\n", size_of(0).unwrap()));
    let before = attributes();
    {
        let _raw = RawMode::enable(0).unwrap().expect("a pty is a terminal");
        let during = Command::new("stty")
            .arg("-a")
            .stdin(Stdio::inherit())
            .output()
            .unwrap()
            .stdout;
        let during = String::from_utf8(during).unwrap();
        report.push(format!(
            "raw {} {}\n",
            during.contains("-echo"),
            during.contains("-icanon")
        ));
    }
    report.push(format!("restored {}\n", attributes() == before));
    let secret = read_secret(0, "Password: ").unwrap();
    report.push(format!("secret {secret}\n"));
    fs::write(path, report.concat()).unwrap();
}

struct Run {
    outcome: String,
    screen: String,
}

/// Runs the test `name` of this binary under a pty and types `typed` once `after` is on the screen.
fn drive(name: &str, after: &str, typed: &[u8]) -> Run {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let outcome_file = dir.path().join("outcome");
    let command = format!(
        "'{}' --exact {name} --nocapture --test-threads=1",
        std::env::current_exe().unwrap().display()
    );
    let mut script = Command::new("script");
    if cfg!(target_os = "linux") {
        script.args(["-qfec", &command, "/dev/null"]);
    } else {
        script.args(["-q", "/dev/null", "sh", "-c", &command]);
    }
    let mut child = script
        .env(CHILD_ENV, &outcome_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let screen = Arc::new(Mutex::new(Vec::<u8>::new()));
    let mut stdout = child.stdout.take().unwrap();
    let sink = Arc::clone(&screen);
    let reader = std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    // Typing earlier would be echoed by the cooked terminal: wait until the child is waiting for it.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !String::from_utf8_lossy(&screen.lock().unwrap()).contains(after) {
        assert!(
            Instant::now() < deadline,
            "the child never showed {after:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(300));
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(typed).unwrap();
    stdin.flush().unwrap();
    let status = child.wait().unwrap();
    drop(stdin);
    reader.join().unwrap();
    assert!(status.success(), "the child failed");
    Run {
        outcome: fs::read_to_string(outcome_file).unwrap(),
        screen: String::from_utf8_lossy(&screen.lock().unwrap()).into_owned(),
    }
}

#[quench_test::test]
fn the_terminal_is_recognised_sized_switched_and_restored_and_a_secret_is_not_echoed() {
    let run = drive(
        "pty::child_asks_about_the_terminal",
        "Password: ",
        b"hunter2\r\n",
    );
    assert_eq!(
        run.outcome,
        "tty true true false\nsize (100, 30)\nraw true true\nrestored true\nsecret hunter2\n"
    );
    assert!(
        !run.screen.contains("hunter2"),
        "the password was echoed: {:?}",
        run.screen
    );
}

#[quench_test::test]
fn a_typed_line_comes_back_and_history_is_browsable() {
    let outcomes = drive(
        "pty::child_reads_one_line",
        "> ",
        b"hello wor\x1b[D\x1b[Dld\r\x1b[A\r",
    )
    .outcome;
    assert_eq!(outcomes, "Line(\"hello wldor\")\nLine(\"from history\")\n");
}

#[quench_test::test]
fn ctrl_c_and_ctrl_d_end_the_read() {
    let outcomes = drive("pty::child_reads_one_line", "> ", b"abc\x03\x04").outcome;
    assert_eq!(outcomes, "Interrupted\nEof\n");
}

#[quench_test::test]
fn a_plain_stdin_falls_back_to_reading_lines() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let outcome_file = dir.path().join("outcome");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pty::child_reads_one_line",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, &outcome_file)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"first line\r\n")
        .unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(
        fs::read_to_string(outcome_file).unwrap(),
        "Line(\"first line\")\nEof\n"
    );
}

quench_test::main!();
