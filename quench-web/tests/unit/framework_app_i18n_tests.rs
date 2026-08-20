//! Unit tests for `framework/app/i18n.rs`.
//!
//! These run with no `i18n/` directory relative to the test binary's working
//! directory (quench-web ships none of its own), so only the "nothing to
//! read yet" branch of each function is reachable here - the same state any
//! consuming service is in before it adds its own `i18n/*.ftl` files.

use quench_web::framework::app::{
    available_locales, generate_translations_js, validate_locales_exist,
};

#[test]
fn available_locales_is_empty_with_no_i18n_directory() {
    assert_eq!(available_locales().unwrap(), Vec::<String>::new());
}

#[test]
fn validating_no_locales_always_succeeds() {
    assert!(validate_locales_exist(&[]).is_ok());
}

#[test]
fn validating_a_locale_with_no_file_fails() {
    let error = validate_locales_exist(&["en-US".to_string()]).unwrap_err();
    assert!(error.to_string().contains("en-US.ftl"));
}

#[test]
fn generate_translations_js_is_an_empty_dictionary_with_no_i18n_directory() {
    let js = generate_translations_js(&["en-US".to_string()]);
    assert_eq!(js, "window.qTranslations = {};");
}
