//! Unit tests for `containers/mod.rs`.

use quench_web::html::builder::elements::{p, span};
use quench_web_components::containers::{
    Card, Container, Panel, card, compact_card, empty_state, highlighted_panel, panel,
};

#[test]
fn a_bare_card_is_just_the_card_div_with_no_optional_sections() {
    let html = Card::new().build().render();
    assert_eq!(html, "<div class=\"card\"></div>");
}

#[test]
fn a_full_card_has_header_body_and_footer_sections() {
    let html = Card::new()
        .title("Title")
        .content(p().text("body"))
        .footer(span().text("footer"))
        .compact(true)
        .build()
        .render();
    assert!(html.contains("class=\"card card-compact\""));
    assert!(html.contains("class=\"card-header\""));
    assert!(html.contains("<h3>Title</h3>"));
    assert!(html.contains("class=\"card-body\""));
    assert!(html.contains(">body<"));
    assert!(html.contains("class=\"card-footer\""));
    assert!(html.contains(">footer<"));
}

#[test]
fn a_bare_panel_is_just_the_panel_div() {
    assert_eq!(Panel::new().build().render(), "<div class=\"panel\"></div>");
}

#[test]
fn a_bordered_highlighted_panel_carries_both_modifier_classes_and_content() {
    let html = Panel::new()
        .title("Notice")
        .content(p().text("careful"))
        .bordered(true)
        .highlighted(true)
        .build()
        .render();
    assert!(html.contains("class=\"panel panel-bordered panel-highlighted\""));
    assert!(html.contains("class=\"panel-header\""));
    assert!(html.contains("<h3>Notice</h3>"));
    assert!(html.contains("class=\"panel-content\""));
    assert!(html.contains(">careful<"));
}

#[test]
fn a_bare_container_is_just_the_container_div() {
    assert_eq!(
        Container::new().build().render(),
        "<div class=\"container\"></div>"
    );
}

#[test]
fn a_fluid_centered_container_carries_both_modifier_classes_and_content() {
    let html = Container::new()
        .content(p().text("x"))
        .fluid(true)
        .centered(true)
        .build()
        .render();
    assert!(html.contains("class=\"container container-fluid container-centered\""));
    assert!(html.contains(">x<"));
}

#[test]
fn card_helper_builds_a_titled_uncompacted_card() {
    let html = card("Title", p().text("body")).render();
    assert!(html.contains("class=\"card\""));
    assert!(!html.contains("card-compact"));
    assert!(html.contains("<h3>Title</h3>"));
}

#[test]
fn compact_card_helper_adds_the_compact_class() {
    assert!(compact_card("T", p()).render().contains("card-compact"));
}

#[test]
fn panel_helper_is_bordered_but_not_highlighted() {
    let html = panel("T", p()).render();
    assert!(html.contains("panel-bordered"));
    assert!(!html.contains("panel-highlighted"));
}

#[test]
fn highlighted_panel_helper_is_both_bordered_and_highlighted() {
    let html = highlighted_panel("T", p()).render();
    assert!(html.contains("panel-bordered"));
    assert!(html.contains("panel-highlighted"));
}

#[test]
fn empty_state_is_a_labelled_placeholder_div() {
    // `Element`'s attributes are a HashMap, so `class`/`data-i18n` can render
    // in either order - checked independently rather than as one substring.
    let html = empty_state("ui_no_runs").render();
    assert!(html.starts_with("<div"));
    assert!(html.contains("class=\"empty\""));
    assert!(html.contains("data-i18n=\"ui_no_runs\""));
    assert!(html.ends_with("></div>"));
}
