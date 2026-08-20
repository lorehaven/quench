//! Unit tests for `framework/app/shell.rs`.
//!
//! `AppShellBuilder::build` writes the estate's static assets to `dist/`
//! relative to the working directory (`assets.rs`'s job, called from here) -
//! that's real, and gitignored, not a test artifact to work around.

use quench_web::framework::app::AppShellBuilder;
use quench_web::framework::theme::Theme;
use quench_web::html::builder::elements::div;

#[test]
fn default_build_has_a_header_with_nav_and_a_footer() {
    let page = AppShellBuilder::new().build().page(div().text("content"));

    assert!(page.contains("<header"));
    assert!(page.contains("q-shell-nav-trigger"));
    assert!(page.contains("<footer"));
    assert!(page.contains("content"));
}

#[test]
fn with_header_false_omits_the_header_entirely() {
    let page = AppShellBuilder::new()
        .with_header(false)
        .build()
        .page(div());

    assert!(!page.contains("<header"));
}

#[test]
fn with_nav_false_keeps_the_header_but_drops_the_nav_trigger() {
    let page = AppShellBuilder::new().with_nav(false).build().page(div());

    assert!(page.contains("<header"));
    assert!(!page.contains("q-shell-nav-trigger"));
}

#[test]
fn an_unsupported_default_theme_falls_back_to_the_first_supported_one() {
    let page = AppShellBuilder::new()
        .supported_themes(vec![Theme::BootstrapDark, Theme::BootstrapLight])
        .default_theme(Theme::DefaultDark)
        .build()
        .page(div());

    // BootstrapDark, the first of the two supported themes, wins instead.
    // `Element`'s attributes are a HashMap, so `href`/`id` can render in
    // either order - checked independently rather than as one substring.
    assert!(page.contains("id=\"theme-link\""));
    assert!(page.contains("themes/bootstrap-dark.css"));
    assert!(!page.contains("themes/default-dark.css\""));
}

#[test]
#[should_panic(expected = "failed to build app shell")]
fn build_panics_when_an_unsupported_locale_file_is_missing() {
    AppShellBuilder::new()
        .supported_locales(vec!["xx-XX".to_string()])
        .build();
}

#[test]
fn try_build_reports_the_missing_locale_file_as_an_error_instead_of_panicking() {
    let error = AppShellBuilder::new()
        .supported_locales(vec!["xx-XX".to_string()])
        .try_build()
        .unwrap_err();
    assert!(error.to_string().contains("xx-XX.ftl"));
}
