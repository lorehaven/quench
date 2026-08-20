//! Unit tests for `buttons/mod.rs`.

use quench_web_components::buttons::{
    ButtonBuilder, ButtonVariant, danger_button, outline_button, primary_button, secondary_button,
    success_button, warning_button,
};

#[test]
fn every_variant_has_its_own_class_name() {
    assert_eq!(ButtonVariant::Primary.class_name(), "btn btn-primary");
    assert_eq!(ButtonVariant::Secondary.class_name(), "btn btn-secondary");
    assert_eq!(ButtonVariant::Danger.class_name(), "btn btn-danger");
    assert_eq!(ButtonVariant::Success.class_name(), "btn btn-success");
    assert_eq!(ButtonVariant::Warning.class_name(), "btn btn-warning");
    assert_eq!(ButtonVariant::Outline.class_name(), "btn btn-outline");
}

#[test]
fn a_plain_button_defaults_to_type_button_and_the_bare_btn_class() {
    let html = ButtonBuilder::new("Save").build().render();
    assert!(html.contains("type=\"button\""));
    assert!(html.contains("class=\"btn\""));
    assert!(html.contains(">Save<"));
    assert!(!html.contains("disabled"));
}

#[test]
fn variant_overrides_the_bare_btn_class() {
    let html = ButtonBuilder::new("Delete")
        .variant(ButtonVariant::Danger)
        .build()
        .render();
    assert!(html.contains("class=\"btn btn-danger\""));
}

#[test]
fn disabled_adds_the_disabled_attribute() {
    let html = ButtonBuilder::new("Save").disabled(true).build().render();
    assert!(html.contains("disabled=\"disabled\""));
}

#[test]
fn button_type_and_id_are_both_configurable() {
    let html = ButtonBuilder::new("Send")
        .button_type("submit")
        .id("send-btn")
        .build()
        .render();
    assert!(html.contains("type=\"submit\""));
    assert!(html.contains("id=\"send-btn\""));
}

#[test]
fn every_convenience_constructor_applies_its_own_variant() {
    assert!(primary_button("x").render().contains("btn-primary"));
    assert!(secondary_button("x").render().contains("btn-secondary"));
    assert!(danger_button("x").render().contains("btn-danger"));
    assert!(success_button("x").render().contains("btn-success"));
    assert!(warning_button("x").render().contains("btn-warning"));
    assert!(outline_button("x").render().contains("btn-outline"));
}
