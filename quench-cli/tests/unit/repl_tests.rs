//! Unit tests for `repl.rs`.
//!
//! `cargo test` runs with a non-interactive stdin (empty/closed), so
//! `rustyline`'s `readline` hits EOF immediately - the same path a real
//! Ctrl-D press takes. That's enough to exercise `run`'s whole loop shape
//! without needing a pty.

use quench_cli::repl::{ReplControl, run};

#[test]
fn run_returns_ok_immediately_on_eof_stdin_without_calling_on_line() {
    let mut calls = 0;
    let result = run("prompt> ", |_line| {
        calls += 1;
        ReplControl::Exit
    });

    assert!(result.is_ok());
    assert_eq!(calls, 0);
}

#[test]
fn repl_control_variants_can_be_constructed() {
    let _continue = ReplControl::Continue("next> ".to_string());
    let _exit = ReplControl::Exit;
}
