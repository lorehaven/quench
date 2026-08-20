//! Unit tests for `components/nav.rs`.

use quench_web::framework::theme::Theme;
use quench_web_components::components::nav::{NavPanelBuilder, nav_button};

#[test]
fn nav_button_toggles_the_modal_and_shows_a_grip_icon() {
    let html = nav_button().render();
    assert!(html.contains("class=\"q-shell-nav-trigger\""));
    assert!(html.contains("onclick="));
    assert!(html.contains("modal-overlay"));
    assert!(html.contains("class=\"fas fa-grip\""));
}

#[test]
fn nav_panel_builder_lists_every_supported_locale_with_its_flag() {
    let html = NavPanelBuilder::new()
        .supported_locales(vec!["en-US".to_string(), "pl-PL".to_string()])
        .default_locale("pl-PL")
        .build()
        .render();

    assert!(html.contains("id=\"locale-select\""));
    assert!(html.contains("🇺🇸 en-US"));
    assert!(html.contains("🇵🇱 pl-PL"));
    assert!(html.contains("value=\"pl-PL\""));
}

#[test]
fn nav_panel_builder_falls_back_to_the_first_locale_when_the_default_is_unsupported() {
    let html = NavPanelBuilder::new()
        .supported_locales(vec!["en-US".to_string(), "de-DE".to_string()])
        .default_locale("fr-FR")
        .build()
        .render();
    assert!(html.contains("value=\"en-US\""));
}

#[test]
fn a_locale_with_no_two_letter_region_gets_no_flag_prefix() {
    let html = NavPanelBuilder::new()
        .supported_locales(vec!["en".to_string()])
        .build()
        .render();
    assert!(html.contains(">en</option>"));
    assert!(!html.contains("> en</option>"));
}

#[test]
fn nav_panel_builder_default_renders_every_theme_variant() {
    let html = NavPanelBuilder::new()
        .supported_locales(vec!["en-US".to_string()])
        .build()
        .render();

    assert!(html.contains("class=\"modal-overlay\""));
    assert!(html.contains("class=\"modal-side q-shell-modal-side\""));
    assert!(html.contains("data-i18n=\"locale_label\""));
    assert!(html.contains("data-i18n=\"theme_label\""));
    assert!(html.contains("id=\"theme-select\""));
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
    assert!(!html.contains("default-dark"));
}
