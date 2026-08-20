//! Unit tests for `framework/components/nav.rs`.

use quench_web::framework::components::nav::{NavPanelBuilder, locale_switch, nav_button};
use quench_web::framework::theme::Theme;

#[test]
fn nav_button_toggles_the_modal_and_shows_a_grip_icon() {
    let html = nav_button().render();
    assert!(html.contains("class=\"q-shell-nav-trigger\""));
    assert!(html.contains("onclick="));
    assert!(html.contains("modal-overlay"));
    assert!(html.contains("class=\"fas fa-grip\""));
}

#[test]
fn locale_switch_lists_every_supported_locale_with_its_flag() {
    let html = locale_switch(
        Some(vec!["en-US".to_string(), "pl-PL".to_string()]),
        Some("pl-PL".to_string()),
    )
    .render();

    assert!(html.contains("class=\"locale-switch q-shell-locale-switch\""));
    assert!(html.contains("id=\"locale-select\""));
    // en-US -> region US -> regional-indicator flag U+1F1FA U+1F1F8.
    assert!(html.contains("🇺🇸 en-US"));
    // pl-PL -> region PL -> U+1F1F5 U+1F1F1.
    assert!(html.contains("🇵🇱 pl-PL"));
    assert!(html.contains("value=\"pl-PL\""));
}

#[test]
fn locale_switch_falls_back_to_the_first_locale_when_the_default_is_not_supported() {
    let html = locale_switch(
        Some(vec!["en-US".to_string(), "de-DE".to_string()]),
        Some("fr-FR".to_string()),
    )
    .render();
    assert!(html.contains("value=\"en-US\""));
}

#[test]
fn a_locale_with_no_two_letter_region_gets_no_flag_prefix() {
    let html = locale_switch(Some(vec!["en".to_string()]), None).render();
    // No region to derive a flag from - the label is just the bare locale,
    // not a leading space plus locale.
    assert!(html.contains(">en</option>"));
    assert!(!html.contains("> en</option>"));
}

#[test]
fn nav_panel_builder_default_renders_locale_and_theme_selects() {
    let html = NavPanelBuilder::new()
        .supported_locales(vec!["en-US".to_string()])
        .build()
        .render();

    assert!(html.contains("class=\"modal-overlay\""));
    assert!(html.contains("class=\"modal-side q-shell-modal-side\""));
    assert!(html.contains("data-i18n=\"locale_label\""));
    assert!(html.contains("data-i18n=\"theme_label\""));
    assert!(html.contains("id=\"theme-select\""));
    // Every `Theme` variant should have its own <option>.
    assert!(html.contains("default-dark"));
    assert!(html.contains("default-light"));
    assert!(html.contains("bootstrap-dark"));
    assert!(html.contains("bootstrap-light"));
}

#[test]
fn nav_panel_builder_respects_an_explicit_theme_list_and_default() {
    let html = NavPanelBuilder::new()
        .supported_locales(vec!["en-US".to_string()])
        .supported_themes(vec![Theme::BootstrapDark, Theme::BootstrapLight])
        .default_theme(Theme::BootstrapLight)
        .build()
        .render();

    assert!(html.contains("value=\"bootstrap-light\""));
    assert!(html.contains("bootstrap-dark"));
    assert!(html.contains("bootstrap-light"));
    assert!(!html.contains("default-dark"));
}
