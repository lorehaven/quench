//! Unit tests for `framework/app/scripts.rs`.
//!
//! These are inline `<script>` bodies, so the interesting Rust-level
//! behaviour is the parameter substitution and fallback logic - not the
//! literal JS text, which is exercised just by rendering at all.

use quench_web::framework::app::{locale_script, session_script, theme_script};
use quench_web::framework::theme::Theme;

#[test]
fn locale_script_uses_the_default_when_it_is_supported() {
    let js = locale_script(&["en-US".to_string(), "pl-PL".to_string()], Some("pl-PL")).render();
    assert!(js.contains("const DEFAULT_LOCALE = \"pl-PL\";"));
}

#[test]
fn locale_script_falls_back_to_the_first_supported_locale() {
    let js = locale_script(&["en-US".to_string(), "pl-PL".to_string()], Some("fr-FR")).render();
    assert!(js.contains("const DEFAULT_LOCALE = \"en-US\";"));
}

#[test]
fn locale_script_falls_back_to_en_us_with_no_supported_locales_at_all() {
    let js = locale_script(&[], None).render();
    assert!(js.contains("const DEFAULT_LOCALE = \"en-US\";"));
}

#[test]
fn session_script_embeds_the_resource_prefixed_urls_and_interval() {
    let js = session_script("/api", 42).render();
    assert!(js.contains("const SESSION_STATUS_URL = \"/api/status\";"));
    assert!(js.contains("const SESSION_REFRESH_URL = \"/api/refresh\";"));
    assert!(js.contains("const SESSION_LOGIN_URL = \"/api/login\";"));
    assert!(js.contains("const SESSION_INTERVAL_MS = 42000;"));
}

#[test]
fn theme_script_lists_every_supported_theme_by_its_css_href() {
    let js = theme_script(
        "default-dark",
        &[Theme::DefaultDark, Theme::BootstrapLight],
        "/api",
    )
    .render();
    assert!(js.contains("const DEFAULT_THEME = \"default-dark\";"));
    assert!(js.contains("\"default-dark\": \"/api/assets/css/themes/default-dark.css\""));
    assert!(js.contains("\"bootstrap-light\": \"/api/assets/css/themes/bootstrap-light.css\""));
}
