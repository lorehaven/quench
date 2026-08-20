//! Unit tests for `framework/components/header.rs`.

use quench_web::framework::components::header::HeaderBuilder;
use quench_web::html::builder::elements::div;

#[test]
fn header_builder_without_nav_omits_the_nav_trigger() {
    let html = HeaderBuilder::new().label("nav_label").build().render();
    assert!(html.starts_with("<header"));
    assert!(html.contains("class=\"q-shell-header\""));
    assert!(html.contains("class=\"left-panel q-shell-header-left\""));
    assert!(html.contains("<h2 data-i18n=\"nav_label\"></h2>"));
    assert!(!html.contains("q-shell-nav-trigger"));
}

#[test]
fn with_nav_adds_the_nav_trigger_and_a_custom_panel() {
    let html = HeaderBuilder::new()
        .label("nav_label")
        .with_nav(div().attr("id", "extra-panel"))
        .build()
        .render();
    assert!(html.contains("q-shell-nav-trigger"));
    assert!(html.contains("id=\"extra-panel\""));
}
