//! `LineEditor::readline` on a real (pseudo) terminal.
//!
//! The editor reads the process's own stdin, so the test re-runs its own binary
//! as a child whose stdin and stdout are the slave side of a pty, types into
//! the master side, and reads the outcome back from a file.
#![cfg(unix)]
#![allow(unsafe_code)]
// pty plumbing

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_cli::editor::LineEditor;
use std::fs;
use std::io::Write;
use std::os::fd::{FromRawFd, OwnedFd, RawFd};
use std::process::{Command, Stdio};

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

fn pty() -> (OwnedFd, OwnedFd) {
    let (mut master, mut slave): (RawFd, RawFd) = (0, 0);
    // SAFETY: `openpty` writes two descriptors into the integers it is given.
    let rc = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    assert_eq!(rc, 0, "openpty failed");
    // SAFETY: both descriptors are fresh and owned by nobody else.
    unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) }
}

fn drive(typed: &[u8]) -> String {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let outcome_file = dir.path().join("outcome");
    let (master, slave) = pty();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pty::child_reads_one_line",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_ENV, &outcome_file)
        .stdin(Stdio::from(slave.try_clone().unwrap()))
        .stdout(Stdio::from(slave.try_clone().unwrap()))
        .stderr(Stdio::from(slave))
        .spawn()
        .unwrap();
    let mut writer = fs::File::from(master);
    // Give the child a moment to enter raw mode before typing; input typed earlier would be echoed by the cooked terminal.
    std::thread::sleep(std::time::Duration::from_millis(500));
    writer.write_all(typed).unwrap();
    writer.flush().unwrap();
    let status = child.wait().unwrap();
    assert!(status.success(), "the child failed");
    fs::read_to_string(outcome_file).unwrap()
}

#[quench_test::test]
fn a_typed_line_comes_back_and_history_is_browsable() {
    let outcomes = drive(b"hello wor\x1b[D\x1b[Dld\r\x1b[A\r");
    assert_eq!(outcomes, "Line(\"hello wldor\")\nLine(\"from history\")\n");
}

#[quench_test::test]
fn ctrl_c_and_ctrl_d_end_the_read() {
    let outcomes = drive(b"abc\x03\x04");
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
