//! Unit tests for `framework/app/i18n.rs`.
//!
//! These run with no `i18n/` directory relative to the test binary's working
//! directory (quench-style ships none of its own), so only the "nothing to
//! read yet" branch of each function is reachable here - the same state any
//! consuming service is in before it adds its own `i18n/*.ftl` files.

use quench_style::i18n::{available_locales, generate_translations_js, validate_locales_exist};

#[quench_test::test]
fn available_locales_is_empty_with_no_i18n_directory() {
    assert_eq!(available_locales().unwrap(), Vec::<String>::new());
}

#[quench_test::test]
fn validating_no_locales_always_succeeds() {
    assert!(validate_locales_exist(&[]).is_ok());
}

#[quench_test::test]
fn validating_a_locale_with_no_file_fails() {
    let error = validate_locales_exist(&["en-US".to_string()]).unwrap_err();
    assert!(error.to_string().contains("en-US.ftl"));
}

#[quench_test::test]
fn generate_translations_js_is_an_empty_dictionary_with_no_i18n_directory() {
    let js = generate_translations_js(&["en-US".to_string()]);
    assert_eq!(js, "window.qTranslations = {};");
}

// With a real directory (these use the `_in` variants: the plain ones read the
// working directory, which tests running in parallel must not change).
mod with_files {
    use quench_core::tmp::TempDir;
    use quench_style::i18n::{available_locales_in, parse_ftl_in, validate_locales_exist_in};
    use std::fs;

    fn bundle() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("pl-PL.ftl"),
            "# comment
greeting = Cześć, { $name }!
plain = Prosty
-term = Ukryty
",
        )
        .unwrap();
        fs::write(
            dir.path().join("en-US.ftl"),
            "greeting = Hello, { $name }!
plain = Plain
z = last
",
        )
        .unwrap();
        fs::write(dir.path().join("notes.txt"), "not a locale").unwrap();
        dir
    }

    #[quench_test::test]
    fn lists_ftl_files_sorted() {
        let dir = bundle();
        assert_eq!(
            available_locales_in(dir.path()).unwrap(),
            ["en-US", "pl-PL"]
        );
        assert!(
            available_locales_in(&dir.path().join("missing"))
                .unwrap()
                .is_empty()
        );
    }

    #[quench_test::test]
    fn validates_against_the_directory() {
        let dir = bundle();
        assert!(validate_locales_exist_in(dir.path(), &["en-US".into(), "pl-PL".into()]).is_ok());
        let e = validate_locales_exist_in(dir.path(), &["fr-FR".into()]).unwrap_err();
        assert!(e.to_string().contains("fr-FR.ftl"), "{e}");
    }

    #[quench_test::test]
    fn translations_are_sorted_json_with_variable_tokens() {
        let dir = bundle();
        let json = parse_ftl_in(dir.path(), None).unwrap();
        assert_eq!(
            json,
            "{\n  \"en-US\": {\n    \"greeting\": \"Hello, {$name}!\",\n    \"plain\": \"Plain\",\n    \"z\": \"last\"\n  },\n  \"pl-PL\": {\n    \"greeting\": \"Cześć, {$name}!\",\n    \"plain\": \"Prosty\"\n  }\n}"
        );
    }

    #[quench_test::test]
    fn only_supported_locales_are_included() {
        let dir = bundle();
        let json = parse_ftl_in(dir.path(), Some(&["pl-PL".to_string()])).unwrap();
        assert!(json.contains("pl-PL") && !json.contains("en-US"), "{json}");
        assert_eq!(parse_ftl_in(dir.path(), Some(&[])).unwrap(), "{}");
    }

    #[quench_test::test]
    fn a_broken_file_names_itself() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("xx.ftl"), "this is not fluent\n").unwrap();
        let e = parse_ftl_in(dir.path(), None).unwrap_err();
        assert!(
            format!("{e:#}").contains("xx.ftl") && format!("{e:#}").contains("expected `=`"),
            "{e:#}"
        );
    }

    #[quench_test::test]
    fn a_missing_directory_is_an_empty_dictionary() {
        let dir = TempDir::new().unwrap();
        assert_eq!(parse_ftl_in(&dir.path().join("nope"), None).unwrap(), "{}");
    }
}
