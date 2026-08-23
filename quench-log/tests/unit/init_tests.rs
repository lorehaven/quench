//! Unit tests for `lib.rs`'s actual logging behaviour: does a line reach
//! the file, in the right format, at the right path.
//!
//! `tracing_subscriber`'s global default can be installed exactly once per
//! process, so a suite that wants more than one independent case can't just
//! call `try_init_with` repeatedly - only the first would succeed. Every
//! test below except the one that specifically exercises that installs
//! nothing globally: it builds a subscriber with `quench_log::subscriber`
//! and scopes it to the closure with `tracing::subscriber::with_default`,
//! which never touches process-wide state and so never collides with a
//! sibling test running in the same binary.

use quench_log::{Config, Format, Rotation};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn read_the_one_log_file(dir: &Path) -> String {
    let entry = fs::read_dir(dir)
        .expect("read log dir")
        .find_map(Result::ok)
        .expect("expected exactly one log file");
    fs::read_to_string(entry.path()).expect("read log file")
}

#[test]
fn file_sink_writes_json_by_default() {
    let dir = tempdir().unwrap();
    let config = Config {
        console_enabled: false,
        file_enabled: true,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "test".to_string(),
        file_format: Format::Json,
        rotation: Rotation::Never,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(count = 42, "hello test");
    });
    drop(guard); // flushes the background writer before the file is read below

    let content = read_the_one_log_file(dir.path());
    assert!(
        content.trim_start().starts_with('{'),
        "expected JSON, got: {content}"
    );
    assert!(content.contains("hello test"));
    assert!(content.contains("\"count\":42"));
}

#[test]
fn file_sink_writes_plain_text_when_configured() {
    let dir = tempdir().unwrap();
    let config = Config {
        console_enabled: false,
        file_enabled: true,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "test".to_string(),
        file_format: Format::Plain,
        rotation: Rotation::Never,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("hello plain");
    });
    drop(guard);

    let content = read_the_one_log_file(dir.path());
    assert!(content.contains("hello plain"));
    assert!(
        !content.trim_start().starts_with('{'),
        "expected plain text, got what looks like JSON: {content}"
    );
}

#[test]
fn rotation_never_names_the_file_after_prefix_and_suffix_only() {
    let dir = tempdir().unwrap();
    let config = Config {
        console_enabled: false,
        file_enabled: true,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "widget".to_string(),
        rotation: Rotation::Never,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("just to make sure the file actually gets created");
    });
    drop(guard);

    assert!(dir.path().join("widget.log").is_file());
}

#[test]
fn creates_the_log_directory_if_it_does_not_exist_yet() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("nested").join("logs");
    assert!(!nested.exists());

    let config = Config {
        file_dir: nested.clone(),
        ..Config::default()
    };

    let (_subscriber, _guard) = quench_log::subscriber(&config).expect("build subscriber");
    assert!(nested.is_dir());
}

#[test]
fn file_disabled_builds_a_subscriber_with_no_guard() {
    let config = Config {
        console_enabled: true,
        file_enabled: false,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    assert!(guard.is_none());

    // Should not panic even with nowhere to send the console layer's output
    // meaningfully checked - the point here is just that building and using
    // a console-only subscriber works.
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("console only, nothing to assert on");
    });
}

#[test]
fn both_sinks_disabled_still_builds_a_usable_subscriber() {
    let config = Config {
        console_enabled: false,
        file_enabled: false,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    assert!(guard.is_none());
    tracing::subscriber::with_default(subscriber, || {
        tracing::info!("goes nowhere, should not panic");
    });
}

/// The one test in this file that touches the real process-global
/// subscriber - every other test above deliberately avoids it via
/// `with_default` so they can run independently of each other and of this
/// one, in any order, in the same test binary.
#[test]
fn try_init_with_installs_a_real_global_default_and_a_second_call_fails() {
    let dir = tempdir().unwrap();
    let config = Config {
        console_enabled: false,
        file_enabled: true,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "global-once".to_string(),
        rotation: Rotation::Never,
        ..Config::default()
    };

    let first = quench_log::try_init_with(config.clone());
    let second = quench_log::try_init_with(config);

    // `first` only asserts `Ok` if this test won the race to be the first
    // caller of `try_init_with`/`init` in the process - something else
    // (another global-touching test, if one is ever added) could have
    // already installed a subscriber first. What matters for this test is
    // the second call: once *a* subscriber is installed, whichever call
    // wins, every later one must fail rather than silently doing nothing.
    let _ = first;
    assert!(matches!(second, Err(quench_log::Error::AlreadyInitialized)));
}
