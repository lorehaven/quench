//! Unit tests for `lib.rs`'s actual logging behaviour: does a line reach
//! the file, in the right format, at the right path.
//!
//! The global dispatcher can be installed exactly once per process, so a
//! suite that wants more than one independent case can't just call
//! `try_init_with` repeatedly - only the first would succeed. Every test
//! below except the one that specifically exercises that installs nothing
//! globally: it builds a dispatcher with `quench_log::subscriber` and scopes
//! it to the closure with `quench_log::with_default`, which never touches
//! process-wide state and so never collides with a sibling test running in
//! the same binary.

use quench_core::tmp::TempDir;
use quench_log::{Config, Format, Rotation};
use std::fs;
use std::path::Path;

fn read_the_one_log_file(dir: &Path) -> String {
    let entry = fs::read_dir(dir)
        .expect("read log dir")
        .find_map(Result::ok)
        .expect("expected exactly one log file");
    fs::read_to_string(entry.path()).expect("read log file")
}

#[quench_test::test]
fn file_sink_writes_json_by_default() {
    let dir = TempDir::new().unwrap();
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
    quench_log::with_default(subscriber, || {
        quench_log::info!(count = 42, "hello test");
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

#[quench_test::test]
fn file_sink_writes_plain_text_when_configured() {
    let dir = TempDir::new().unwrap();
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
    quench_log::with_default(subscriber, || {
        quench_log::info!("hello plain");
    });
    drop(guard);

    let content = read_the_one_log_file(dir.path());
    assert!(content.contains("hello plain"));
    assert!(
        !content.trim_start().starts_with('{'),
        "expected plain text, got what looks like JSON: {content}"
    );
}

#[quench_test::test]
fn rotation_never_names_the_file_after_prefix_and_suffix_only() {
    let dir = TempDir::new().unwrap();
    let config = Config {
        console_enabled: false,
        file_enabled: true,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "widget".to_string(),
        rotation: Rotation::Never,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    quench_log::with_default(subscriber, || {
        quench_log::info!("just to make sure the file actually gets created");
    });
    drop(guard);

    assert!(dir.path().join("widget.log").is_file());
}

#[quench_test::test]
fn creates_the_log_directory_if_it_does_not_exist_yet() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("nested").join("logs");
    assert!(!nested.exists());

    let config = Config {
        file_dir: nested.clone(),
        ..Config::default()
    };

    let (_subscriber, _guard) = quench_log::subscriber(&config).expect("build subscriber");
    assert!(nested.is_dir());
}

#[quench_test::test]
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
    quench_log::with_default(subscriber, || {
        quench_log::info!("console only, nothing to assert on");
    });
}

#[quench_test::test]
fn both_sinks_disabled_still_builds_a_usable_subscriber() {
    let config = Config {
        console_enabled: false,
        file_enabled: false,
        ..Config::default()
    };

    let (subscriber, guard) = quench_log::subscriber(&config).expect("build subscriber");
    assert!(guard.is_none());
    quench_log::with_default(subscriber, || {
        quench_log::info!("goes nowhere, should not panic");
    });
}

/// The one test in this file that touches the real process-global
/// subscriber - every other test above deliberately avoids it via
/// `with_default` so they can run independently of each other and of this
/// one, in any order, in the same test binary.
#[quench_test::test]
fn try_init_with_installs_a_real_global_default_and_a_second_call_fails() {
    let dir = TempDir::new().unwrap();
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

#[quench_test::test]
fn the_directory_error_names_the_path() {
    let dir = TempDir::new().unwrap();
    let blocker = dir.path().join("file");
    fs::write(&blocker, "x").unwrap();
    let config = Config {
        file_dir: blocker.join("under-a-file"),
        ..Config::default()
    };
    let error = quench_log::subscriber(&config).unwrap_err();
    assert!(matches!(error, quench_log::Error::CreateLogDir { .. }));
    assert!(error.to_string().contains("under-a-file"), "{error}");
}

#[quench_test::test]
fn an_unparseable_level_falls_back_to_info() {
    let dir = TempDir::new().unwrap();
    let config = Config {
        console_enabled: false,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "fallback".to_string(),
        rotation: Rotation::Never,
        level: "this is not a filter".to_string(),
        ..Config::default()
    };
    let (subscriber, guard) = quench_log::subscriber(&config).unwrap();
    quench_log::with_default(subscriber, || {
        quench_log::debug!("hidden at info");
        quench_log::info!("shown at info");
    });
    drop(guard);
    let content = read_the_one_log_file(dir.path());
    assert!(
        content.contains("shown at info") && !content.contains("hidden at info"),
        "{content}"
    );
}

#[quench_test::test]
fn the_filter_decides_what_reaches_the_file() {
    let dir = TempDir::new().unwrap();
    let config = Config {
        console_enabled: false,
        file_dir: dir.path().to_path_buf(),
        file_prefix: "filtered".to_string(),
        file_format: Format::Plain,
        rotation: Rotation::Never,
        level: "warn".to_string(),
        ..Config::default()
    };
    let (subscriber, guard) = quench_log::subscriber(&config).unwrap();
    quench_log::with_default(subscriber, || {
        quench_log::info!("too quiet");
        quench_log::warn!("loud enough");
        quench_log::error!(code = 500, "worse");
    });
    drop(guard);
    let content = read_the_one_log_file(dir.path());
    assert!(
        !content.contains("too quiet")
            && content.contains("loud enough")
            && content.contains("code=500"),
        "{content}"
    );
    assert_eq!(content.lines().count(), 2);
}

#[quench_test::test]
fn init_with_panics_on_a_second_install_and_names_the_crate() {
    let dir = TempDir::new().unwrap();
    let config = Config {
        console_enabled: false,
        file_dir: dir.path().to_path_buf(),
        rotation: Rotation::Never,
        ..Config::default()
    };
    let _ = quench_log::try_init_with(config.clone());
    let result = std::panic::catch_unwind(|| quench_log::init_with(config));
    let message = result
        .unwrap_err()
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(
        message.starts_with("quench-log:") && message.contains("already installed"),
        "{message}"
    );
    let _ = quench_log::global_is_set();
}

#[quench_test::test]
fn error_variants_describe_themselves() {
    use std::error::Error as _;
    let create = quench_log::Error::CreateLogDir {
        path: "/logs".into(),
        source: std::io::Error::other("denied"),
    };
    assert_eq!(
        create.to_string(),
        "could not create log directory /logs: denied"
    );
    assert!(create.source().is_some());
    let appender = quench_log::Error::from(std::io::Error::other("disk full"));
    assert_eq!(
        appender.to_string(),
        "could not set up the rolling file appender: disk full"
    );
    assert!(appender.source().is_some());
    let twice = quench_log::Error::AlreadyInitialized;
    assert!(twice.to_string().contains("already installed"));
    assert!(twice.source().is_none());
    assert!(format!("{twice:?}").contains("AlreadyInitialized"));
}
