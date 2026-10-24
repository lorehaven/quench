//! Unit tests for `config.rs`. Everything here is a pure function or a
//! `Default` - no env var to set and restore, so nothing here needs to
//! serialize against other tests the way `init_tests.rs` does.

use quench_log::config::{parse_bool, resolve_file_prefix};
use quench_log::{Config, Format, Rotation};
use std::path::PathBuf;

#[quench_test::test]
fn default_config_is_console_and_file_both_on_daily_json_seven_files() {
    let config = Config::default();
    assert!(config.console_enabled);
    assert!(config.file_enabled);
    assert_eq!(config.level, "info");
    assert_eq!(config.file_dir, PathBuf::from("logs"));
    assert_eq!(config.file_prefix, "app");
    assert_eq!(config.file_format, Format::Json);
    assert_eq!(config.rotation, Rotation::Daily);
    assert_eq!(config.max_files, 7);
}

// ---------------------------------------------------------------------------
// Format
// ---------------------------------------------------------------------------

#[quench_test::test]
fn format_parses_json_and_plain() {
    assert_eq!(Format::parse("json"), Some(Format::Json));
    assert_eq!(Format::parse("JSON"), Some(Format::Json));
    assert_eq!(Format::parse("plain"), Some(Format::Plain));
    assert_eq!(Format::parse("text"), Some(Format::Plain));
    assert_eq!(Format::parse("  Plain  "), Some(Format::Plain));
}

#[quench_test::test]
fn format_rejects_anything_else() {
    assert_eq!(Format::parse("yaml"), None);
    assert_eq!(Format::parse(""), None);
}

#[quench_test::test]
fn format_round_trips_through_as_str() {
    for format in [Format::Json, Format::Plain] {
        assert_eq!(Format::parse(format.as_str()), Some(format));
    }
}

#[quench_test::test]
fn format_display_matches_as_str() {
    assert_eq!(Format::Json.to_string(), "json");
    assert_eq!(Format::Plain.to_string(), "plain");
}

// ---------------------------------------------------------------------------
// Rotation
// ---------------------------------------------------------------------------

#[quench_test::test]
fn rotation_parses_every_documented_spelling() {
    assert_eq!(Rotation::parse("daily"), Some(Rotation::Daily));
    assert_eq!(Rotation::parse("Hourly"), Some(Rotation::Hourly));
    assert_eq!(Rotation::parse(" never "), Some(Rotation::Never));
}

#[quench_test::test]
fn rotation_rejects_anything_else() {
    assert_eq!(Rotation::parse("weekly"), None);
}

#[quench_test::test]
fn rotation_round_trips_through_as_str() {
    for rotation in [Rotation::Daily, Rotation::Hourly, Rotation::Never] {
        assert_eq!(Rotation::parse(rotation.as_str()), Some(rotation));
    }
}

// ---------------------------------------------------------------------------
// parse_bool
// ---------------------------------------------------------------------------

#[quench_test::test]
fn parse_bool_accepts_the_usual_true_spellings() {
    for value in ["1", "true", "True", "yes", "on", "  TRUE  "] {
        assert_eq!(parse_bool(value), Some(true), "{value:?}");
    }
}

#[quench_test::test]
fn parse_bool_accepts_the_usual_false_spellings() {
    for value in ["0", "false", "False", "no", "off"] {
        assert_eq!(parse_bool(value), Some(false), "{value:?}");
    }
}

#[quench_test::test]
fn parse_bool_rejects_anything_else() {
    assert_eq!(parse_bool("maybe"), None);
    assert_eq!(parse_bool(""), None);
}

// ---------------------------------------------------------------------------
// resolve_file_prefix
// ---------------------------------------------------------------------------

#[quench_test::test]
fn resolve_file_prefix_prefers_the_explicit_value() {
    assert_eq!(
        resolve_file_prefix(Some("conveyor"), Some("cargo-bin-name")),
        "conveyor"
    );
}

#[quench_test::test]
fn resolve_file_prefix_falls_back_to_cargo_bin_name() {
    assert_eq!(
        resolve_file_prefix(None, Some("warehouse-service")),
        "warehouse-service"
    );
}

#[quench_test::test]
fn resolve_file_prefix_falls_back_to_app_with_neither() {
    assert_eq!(resolve_file_prefix(None, None), "app");
}

#[quench_test::test]
fn resolve_file_prefix_treats_a_blank_explicit_value_as_absent() {
    // A blank `LOG_FILE_PREFIX=""` should not win over a real
    // `CARGO_BIN_NAME` - and if there is none either, it should not produce
    // a file whose name starts with nothing.
    assert_eq!(
        resolve_file_prefix(Some("   "), Some("sage-service")),
        "sage-service"
    );
    assert_eq!(resolve_file_prefix(Some("   "), None), "app");
}

// ---------------------------------------------------------------------------
// The environment. One test owns every variable it touches, set and cleared in
// sequence: tests in a binary run on parallel threads, and the process
// environment is shared between them.
// ---------------------------------------------------------------------------

const VARS: [&str; 9] = [
    "LOG_CONSOLE_ENABLED",
    "LOG_FILE_ENABLED",
    "RUST_LOG",
    "LOG_DIR",
    "LOG_FILE_PREFIX",
    "CARGO_BIN_NAME",
    "LOG_FILE_FORMAT",
    "LOG_ROTATION",
    "LOG_MAX_FILES",
];

fn clear_env() {
    for var in VARS {
        quench_core::env::remove(var);
    }
}

#[quench_test::test]
fn from_env_reads_every_variable_and_falls_back_to_the_defaults() {
    clear_env();
    // Nothing set (CARGO_BIN_NAME is set by cargo for binaries, not for tests): defaults.
    assert_eq!(
        Config::from_env(),
        Config {
            file_prefix: Config::default().file_prefix,
            ..Config::default()
        }
    );

    for (name, value) in [
        ("LOG_CONSOLE_ENABLED", "off"),
        ("LOG_FILE_ENABLED", "no"),
        ("RUST_LOG", "debug,hyper=warn"),
        ("LOG_DIR", "/var/log/svc"),
        ("LOG_FILE_PREFIX", "  svc  "),
        ("LOG_FILE_FORMAT", "plain"),
        ("LOG_ROTATION", "hourly"),
        ("LOG_MAX_FILES", "30"),
    ] {
        quench_core::env::set(name, value);
    }
    let config = Config::from_env();
    assert!(!config.console_enabled && !config.file_enabled);
    assert_eq!(config.level, "debug,hyper=warn");
    assert_eq!(config.file_dir, PathBuf::from("/var/log/svc"));
    assert_eq!(config.file_prefix, "svc");
    assert_eq!(config.file_format, Format::Plain);
    assert_eq!(config.rotation, Rotation::Hourly);
    assert_eq!(config.max_files, 30);

    // the binary name stands in for a missing prefix; junk values fall back
    quench_core::env::remove("LOG_FILE_PREFIX");
    quench_core::env::set("CARGO_BIN_NAME", "widget");
    quench_core::env::set("LOG_FILE_FORMAT", "yaml");
    quench_core::env::set("LOG_ROTATION", "weekly");
    quench_core::env::set("LOG_MAX_FILES", "many");
    quench_core::env::set("LOG_CONSOLE_ENABLED", "maybe");
    quench_core::env::set("RUST_LOG", "   ");
    let config = Config::from_env();
    assert_eq!(config.file_prefix, "widget");
    assert_eq!(config.file_format, Format::default());
    assert_eq!(config.rotation, Rotation::default());
    assert_eq!(config.max_files, 7);
    assert!(config.console_enabled);
    assert_eq!(config.level, "info");

    // `init()` reads this environment; with both sinks off it installs a
    // dispatcher that writes nothing, and a second `init()` is refused.
    quench_core::env::set("LOG_CONSOLE_ENABLED", "false");
    quench_core::env::set("LOG_FILE_ENABLED", "false");
    let first = std::panic::catch_unwind(quench_log::init);
    let second = std::panic::catch_unwind(quench_log::init);
    clear_env();
    // Another test in this binary may have installed the global dispatcher first.
    let _ = first;
    let message = second
        .unwrap_err()
        .downcast_ref::<String>()
        .cloned()
        .unwrap_or_default();
    assert!(message.contains("already installed"), "{message}");
}

#[quench_test::test]
fn names_display_as_their_config_spelling() {
    assert_eq!(Format::Json.to_string(), "json");
    assert_eq!(Format::Plain.to_string(), "plain");
    assert_eq!(Rotation::Daily.to_string(), "daily");
    assert_eq!(Rotation::Hourly.to_string(), "hourly");
    assert_eq!(Rotation::Never.to_string(), "never");
    assert_eq!(Format::parse("text"), Some(Format::Plain));
    assert_eq!(Rotation::parse(" Never "), Some(Rotation::Never));
}

#[quench_test::test]
fn config_and_names_are_plain_values() {
    let config = Config::default();
    let copy = config.clone();
    assert_eq!(copy, config);
    assert_ne!(
        copy,
        Config {
            max_files: 99,
            ..Config::default()
        }
    );
    assert!(format!("{config:?}").contains("max_files: 7"));
    assert_ne!(Format::Json, Format::Plain);
    assert_eq!(
        format!("{:?} {:?}", Format::Json, Format::Plain),
        "Json Plain"
    );
    assert_eq!(Rotation::default(), Rotation::Daily);
    assert_eq!(format!("{:?}", Rotation::Never), "Never");
}
