//! Unit tests for `framework/theme/mod.rs`.

use quench_web::framework::theme::{Theme, theme_shared};

#[test]
fn each_theme_display_matches_its_documented_slug() {
    assert_eq!(Theme::DefaultDark.to_string(), "default-dark");
    assert_eq!(Theme::DefaultLight.to_string(), "default-light");
    assert_eq!(Theme::BootstrapDark.to_string(), "bootstrap-dark");
    assert_eq!(Theme::BootstrapLight.to_string(), "bootstrap-light");
}

#[test]
fn theme_dispatches_to_the_matching_stylesheet() {
    // Each theme's own accent colour, to prove `Theme::theme` reached the
    // right implementation rather than always the same one.
    assert!(Theme::theme(Theme::DefaultDark).contains("#be123c"));
    assert!(Theme::theme(Theme::DefaultLight).contains("#be123c"));
    assert!(Theme::theme(Theme::BootstrapDark).contains("#ea868f"));
    assert!(Theme::theme(Theme::BootstrapLight).contains("#dc3545"));
}

#[test]
fn theme_shared_concatenates_every_shared_style_module() {
    let shared = theme_shared();
    // One selector from each of root/header/content/footer/elements/modal,
    // proving every module theme_shared draws from actually contributed.
    assert!(shared.contains(":root {"));
    assert!(shared.contains("header,\n.q-shell-header {"));
    assert!(shared.contains(".content,\n.q-shell-content {"));
    assert!(shared.contains("footer,\n.q-shell-footer {"));
    assert!(shared.contains("h1 {"));
    assert!(shared.contains(".modal-overlay {"));
}
