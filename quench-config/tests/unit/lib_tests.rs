//! Unit tests for `lib.rs`.

use quench_config::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Sample {
    name: String,
    count: u32,
}

/// Writes `contents` to a uniquely-named file in the OS temp dir with the
/// given extension, returning its path. Each test picks its own file name so
/// parallel test threads never collide.
fn temp_file(name: &str, ext: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "quench-config-test-{name}-{}.{ext}",
        std::process::id()
    ));
    std::fs::write(&path, contents).expect("write temp file");
    path
}

#[test]
fn test_config_loader_env() {
    unsafe {
        std::env::set_var("TEST_VAR", "test_value");
    }
    let loader = ConfigLoader::new("TEST");
    let value = loader.env_string("VAR", "default");
    assert_eq!(value, "test_value");
}

#[test]
fn test_config_loader_env_list() {
    unsafe {
        std::env::set_var("TEST_LIST", "item1, item2, item3");
    }
    let loader = ConfigLoader::new("TEST");
    let list = loader.env_list("LIST", &[]);
    assert_eq!(list.len(), 3);
    assert_eq!(list[0], "item1");
}

#[test]
fn test_config_loader_bool() {
    unsafe {
        std::env::set_var("TEST_ENABLED", "true");
    }
    let loader = ConfigLoader::new("TEST");
    assert!(loader.env_bool("ENABLED", false));

    unsafe {
        std::env::set_var("TEST_DISABLED", "false");
    }
    assert!(!loader.env_bool("DISABLED", true));
}

#[test]
fn env_string_falls_back_to_the_default_when_neither_prefixed_nor_bare_key_is_set() {
    let loader = ConfigLoader::new("QCFG");
    let value = loader.env_string("TOTALLY_UNSET_KEY", "fallback");
    assert_eq!(value, "fallback");
}

#[test]
fn env_string_falls_back_to_the_bare_key_when_the_prefixed_one_is_absent() {
    unsafe {
        std::env::set_var("BARE_ONLY_KEY", "bare-value");
    }
    let loader = ConfigLoader::new("QCFG");
    assert_eq!(loader.env_string("BARE_ONLY_KEY", "default"), "bare-value");
}

#[test]
fn env_u64_parses_a_valid_number_and_falls_back_on_a_bad_one() {
    unsafe {
        std::env::set_var("TEST_PORT", "9090");
    }
    let loader = ConfigLoader::new("TEST");
    assert_eq!(loader.env_u64("PORT", 8080), 9090);

    unsafe {
        std::env::set_var("TEST_NOT_A_NUMBER", "not-a-number");
    }
    assert_eq!(loader.env_u64("NOT_A_NUMBER", 42), 42);
}

#[test]
fn env_bool_recognises_every_truthy_spelling_case_insensitively() {
    let loader = ConfigLoader::new("TEST");
    for (key, value) in [
        ("TRUTHY_ONE", "1"),
        ("TRUTHY_YES", "YES"),
        ("TRUTHY_ON", "On"),
    ] {
        unsafe {
            std::env::set_var(format!("TEST_{key}"), value);
        }
        assert!(loader.env_bool(key, false), "{value} should be truthy");
    }
}

#[test]
fn env_bool_treats_an_unrecognised_present_value_as_false_regardless_of_default() {
    unsafe {
        std::env::set_var("TEST_GIBBERISH", "not-a-bool");
    }
    let loader = ConfigLoader::new("TEST");
    assert!(!loader.env_bool("GIBBERISH", true));
}

#[test]
fn env_bool_falls_back_to_the_default_when_the_key_is_entirely_unset() {
    let loader = ConfigLoader::new("TEST");
    assert!(!loader.env_bool("GIBBERISH_UNSET", false));
    assert!(loader.env_bool("GIBBERISH_UNSET_2", true));
}

#[test]
fn env_list_falls_back_to_the_default_list_and_trims_and_drops_empty_entries() {
    let loader = ConfigLoader::new("QCFG");
    let list = loader.env_list("UNSET_LIST_KEY", &["a", "b"]);
    assert_eq!(list, vec!["a".to_string(), "b".to_string()]);

    unsafe {
        std::env::set_var("TEST_MESSY_LIST", " one ,, two ,");
    }
    let list = ConfigLoader::new("TEST").env_list("MESSY_LIST", &[]);
    assert_eq!(list, vec!["one".to_string(), "two".to_string()]);
}

#[test]
fn from_json_file_round_trips_a_valid_document_and_errors_on_a_bad_path() {
    let path = temp_file("json-ok", "json", r#"{"name":"svc","count":3}"#);
    let loaded: Sample = ConfigLoader::from_json_file(path.to_str().unwrap()).expect("load json");
    assert_eq!(
        loaded,
        Sample {
            name: "svc".to_string(),
            count: 3
        }
    );
    std::fs::remove_file(&path).ok();

    let err = ConfigLoader::from_json_file::<Sample>("/no/such/path.json").unwrap_err();
    assert!(matches!(err, ConfigError::ReadError(_)));
}

#[test]
fn from_json_file_errors_on_malformed_json() {
    let path = temp_file("json-bad", "json", "{not valid json");
    let err = ConfigLoader::from_json_file::<Sample>(path.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, ConfigError::ParseError(_)));
    std::fs::remove_file(&path).ok();
}

#[test]
fn from_yaml_file_round_trips_a_valid_document_and_errors_on_bad_yaml() {
    let path = temp_file("yaml-ok", "yaml", "name: svc\ncount: 5\n");
    let loaded: Sample = ConfigLoader::from_yaml_file(path.to_str().unwrap()).expect("load yaml");
    assert_eq!(
        loaded,
        Sample {
            name: "svc".to_string(),
            count: 5
        }
    );
    std::fs::remove_file(&path).ok();

    let bad_path = temp_file("yaml-bad", "yaml", ":::not yaml:::");
    let err = ConfigLoader::from_yaml_file::<Sample>(bad_path.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, ConfigError::ParseError(_)));
    std::fs::remove_file(&bad_path).ok();
}

#[test]
fn from_toml_file_round_trips_a_valid_document_and_errors_on_bad_toml() {
    let path = temp_file("toml-ok", "toml", "name = \"svc\"\ncount = 7\n");
    let loaded: Sample = ConfigLoader::from_toml_file(path.to_str().unwrap()).expect("load toml");
    assert_eq!(
        loaded,
        Sample {
            name: "svc".to_string(),
            count: 7
        }
    );
    std::fs::remove_file(&path).ok();

    let bad_path = temp_file("toml-bad", "toml", "not = = valid");
    let err = ConfigLoader::from_toml_file::<Sample>(bad_path.to_str().unwrap()).unwrap_err();
    assert!(matches!(err, ConfigError::ParseError(_)));
    std::fs::remove_file(&bad_path).ok();
}

#[test]
fn load_with_fallback_reads_from_a_json_file_when_the_path_exists() {
    let path = temp_file("fallback-json", "json", r#"{"name":"from-file","count":1}"#);
    let loaded: Sample = ConfigLoader::load_with_fallback(
        Some(path.to_str().unwrap()),
        "UNUSED_ENV_VAR_FOR_FALLBACK_JSON",
        None,
    )
    .expect("load from file");
    assert_eq!(loaded.name, "from-file");
    std::fs::remove_file(&path).ok();
}

#[test]
fn load_with_fallback_reads_from_a_yaml_file_by_extension() {
    let path = temp_file("fallback-yaml", "yaml", "name: from-yaml\ncount: 2\n");
    let loaded: Sample = ConfigLoader::load_with_fallback(
        Some(path.to_str().unwrap()),
        "UNUSED_ENV_VAR_FOR_FALLBACK_YAML",
        None,
    )
    .expect("load from file");
    assert_eq!(loaded.name, "from-yaml");
    std::fs::remove_file(&path).ok();
}

#[test]
fn load_with_fallback_reads_from_a_toml_file_by_extension() {
    let path = temp_file("fallback-toml", "toml", "name = \"from-toml\"\ncount = 4\n");
    let loaded: Sample = ConfigLoader::load_with_fallback(
        Some(path.to_str().unwrap()),
        "UNUSED_ENV_VAR_FOR_FALLBACK_TOML",
        None,
    )
    .expect("load from file");
    assert_eq!(loaded.name, "from-toml");
    std::fs::remove_file(&path).ok();
}

#[test]
fn load_with_fallback_falls_back_to_the_env_var_when_no_file_path_is_given() {
    unsafe {
        std::env::set_var("FALLBACK_ENV_ONLY", r#"{"name":"from-env","count":9}"#);
    }
    let loaded: Sample =
        ConfigLoader::load_with_fallback(None, "FALLBACK_ENV_ONLY", None).expect("load from env");
    assert_eq!(loaded.name, "from-env");
}

#[test]
fn load_with_fallback_falls_back_to_the_env_var_when_the_file_path_does_not_exist() {
    unsafe {
        std::env::set_var(
            "FALLBACK_ENV_OVER_MISSING_FILE",
            r#"{"name":"from-env-2","count":10}"#,
        );
    }
    let loaded: Sample = ConfigLoader::load_with_fallback(
        Some("/no/such/file.json"),
        "FALLBACK_ENV_OVER_MISSING_FILE",
        None,
    )
    .expect("load from env");
    assert_eq!(loaded.name, "from-env-2");
}

#[test]
fn load_with_fallback_uses_the_default_when_no_file_or_env_var_is_available() {
    let default = Sample {
        name: "default".to_string(),
        count: 0,
    };
    let loaded: Sample = ConfigLoader::load_with_fallback(
        None,
        "TOTALLY_UNSET_FALLBACK_ENV_VAR",
        Some(Sample {
            name: "default".to_string(),
            count: 0,
        }),
    )
    .expect("load default");
    assert_eq!(loaded, default);
}

#[test]
fn load_with_fallback_errors_when_nothing_is_available_and_there_is_no_default() {
    let err = ConfigLoader::load_with_fallback::<Sample>(
        None,
        "TOTALLY_UNSET_FALLBACK_ENV_VAR_NO_DEFAULT",
        None,
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::MissingValue(_)));
}

#[test]
fn app_config_from_env_reads_every_field_with_its_prefix() {
    unsafe {
        std::env::set_var("APP_SERVICE_NAME", "my-service");
        std::env::set_var("APP_PORT", "3000");
        std::env::set_var("APP_DATABASE_URL", "postgres://localhost/db");
        std::env::set_var("APP_LOG_LEVEL", "debug");
        std::env::set_var("APP_DEBUG", "true");
    }
    let config = AppConfig::from_env();
    assert_eq!(config.service_name, "my-service");
    assert_eq!(config.port, 3000);
    assert_eq!(config.database_url, "postgres://localhost/db");
    assert_eq!(config.log_level, "debug");
    assert!(config.debug);
}
