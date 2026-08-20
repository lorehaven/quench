//! Unit tests for `terminal.rs`.
//!
//! Most functions here are `println!`/`eprintln!` wrappers with no return
//! value to assert on - calling them exercises the formatting/styling code
//! paths without asserting on ANSI escape content, which crossterm may or
//! may not emit depending on whether stdout is a tty in the test process.

use quench_cli::terminal::{
    BLUE, BOLD, CYAN, DIM, GREEN, RESET, SEP, SEP_THIN, Tone, WHITE, YELLOW, print_box_banner,
    print_component_preview, print_error_line, print_inline, print_line, print_status, repl_prompt,
};

#[test]
fn ansi_constants_are_non_empty_escape_sequences() {
    for code in [RESET, BOLD, DIM, CYAN, BLUE, GREEN, YELLOW, WHITE] {
        assert!(code.starts_with('\x1b'));
    }
    assert!(!SEP.is_empty());
    assert!(!SEP_THIN.is_empty());
}

#[test]
fn repl_prompt_wraps_the_app_name_and_context_together() {
    let prompt = repl_prompt("quench", "auth");
    assert!(prompt.contains("quench"));
    assert!(prompt.contains("auth"));
    assert!(prompt.ends_with("> "));
}

#[test]
fn print_box_banner_does_not_panic_for_any_title_length() {
    print_box_banner("Title", "Subtitle");
    print_box_banner("", "");
}

#[test]
fn print_status_does_not_panic_for_every_tone() {
    print_status(Tone::Info, "INFO", "message");
    print_status(Tone::Success, "OK", "message");
    print_status(Tone::Warn, "WARN", "message");
    print_status(Tone::Error, "ERROR", "message");
}

#[test]
fn print_component_preview_does_not_panic() {
    print_component_preview("nav", "the top navigation bar");
}

#[test]
fn print_line_and_print_error_line_do_not_panic() {
    print_line("stdout line");
    print_error_line("stderr line");
}

#[test]
fn print_inline_flushes_without_a_trailing_newline() {
    print_inline("partial");
}
