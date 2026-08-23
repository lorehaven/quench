//! Unit tests for `config.rs`. Everything here is a pure function or a
//! `Default` - no env var to set and restore, so nothing here needs to
//! serialize against other tests the way `init_tests.rs` does.

use quench_log::config::{parse_bool, resolve_file_prefix};
use quench_log::{Config, Format, Rotation};
use std::path::PathBuf;

#[test]
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

#[test]
fn format_parses_json_and_plain() {
    assert_eq!(Format::parse("json"), Some(Format::Json));
    assert_eq!(Format::parse("JSON"), Some(Format::Json));
    assert_eq!(Format::parse("plain"), Some(Format::Plain));
    assert_eq!(Format::parse("text"), Some(Format::Plain));
    assert_eq!(Format::parse("  Plain  "), Some(Format::Plain));
}

#[test]
fn format_rejects_anything_else() {
    assert_eq!(Format::parse("yaml"), None);
    assert_eq!(Format::parse(""), None);
}

#[test]
fn format_round_trips_through_as_str() {
    for format in [Format::Json, Format::Plain] {
        assert_eq!(Format::parse(format.as_str()), Some(format));
    }
}

#[test]
fn format_display_matches_as_str() {
    assert_eq!(Format::Json.to_string(), "json");
    assert_eq!(Format::Plain.to_string(), "plain");
}

// ---------------------------------------------------------------------------
// Rotation
// ---------------------------------------------------------------------------

#[test]
fn rotation_parses_every_documented_spelling() {
    assert_eq!(Rotation::parse("daily"), Some(Rotation::Daily));
    assert_eq!(Rotation::parse("Hourly"), Some(Rotation::Hourly));
    assert_eq!(Rotation::parse(" never "), Some(Rotation::Never));
}

#[test]
fn rotation_rejects_anything_else() {
    assert_eq!(Rotation::parse("weekly"), None);
}

#[test]
fn rotation_round_trips_through_as_str() {
    for rotation in [Rotation::Daily, Rotation::Hourly, Rotation::Never] {
        assert_eq!(Rotation::parse(rotation.as_str()), Some(rotation));
    }
}

// ---------------------------------------------------------------------------
// parse_bool
// ---------------------------------------------------------------------------

#[test]
fn parse_bool_accepts_the_usual_true_spellings() {
    for value in ["1", "true", "True", "yes", "on", "  TRUE  "] {
        assert_eq!(parse_bool(value), Some(true), "{value:?}");
    }
}

#[test]
fn parse_bool_accepts_the_usual_false_spellings() {
    for value in ["0", "false", "False", "no", "off"] {
        assert_eq!(parse_bool(value), Some(false), "{value:?}");
    }
}

#[test]
fn parse_bool_rejects_anything_else() {
    assert_eq!(parse_bool("maybe"), None);
    assert_eq!(parse_bool(""), None);
}

// ---------------------------------------------------------------------------
// resolve_file_prefix
// ---------------------------------------------------------------------------

#[test]
fn resolve_file_prefix_prefers_the_explicit_value() {
    assert_eq!(
        resolve_file_prefix(Some("conveyor"), Some("cargo-bin-name")),
        "conveyor"
    );
}

#[test]
fn resolve_file_prefix_falls_back_to_cargo_bin_name() {
    assert_eq!(
        resolve_file_prefix(None, Some("warehouse-service")),
        "warehouse-service"
    );
}

#[test]
fn resolve_file_prefix_falls_back_to_app_with_neither() {
    assert_eq!(resolve_file_prefix(None, None), "app");
}

#[test]
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
