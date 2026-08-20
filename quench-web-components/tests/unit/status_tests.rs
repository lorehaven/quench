//! Unit tests for `status/mod.rs`.

use quench_web_components::status::{
    AlertBox, StatusBadge, StatusLevel, error_alert, error_badge, info_alert, success_alert,
    success_badge, warning_alert, warning_badge,
};

#[test]
fn every_status_level_has_its_own_class_and_icon() {
    assert_eq!(StatusLevel::Info.class_name(), "status-info");
    assert_eq!(StatusLevel::Success.class_name(), "status-success");
    assert_eq!(StatusLevel::Warning.class_name(), "status-warning");
    assert_eq!(StatusLevel::Error.class_name(), "status-error");

    assert_eq!(StatusLevel::Info.icon_class(), "fas fa-info-circle");
    assert_eq!(StatusLevel::Success.icon_class(), "fas fa-check-circle");
    assert_eq!(
        StatusLevel::Warning.icon_class(),
        "fas fa-exclamation-circle"
    );
    assert_eq!(StatusLevel::Error.icon_class(), "fas fa-times-circle");
}

#[test]
fn a_bare_status_badge_has_no_level_class_or_icon() {
    let html = StatusBadge::new("Draft").build().render();
    assert_eq!(html, "<span class=\"status-badge\">Draft</span>");
}

#[test]
fn a_levelled_status_badge_carries_the_level_class_but_no_icon_by_default() {
    let html = StatusBadge::new("Live")
        .level(StatusLevel::Success)
        .build()
        .render();
    assert!(html.contains("class=\"status-badge status-success\""));
    assert!(!html.contains("<i"));
    assert!(html.contains(">Live<"));
}

#[test]
fn with_icon_adds_the_icon_element_only_when_a_level_is_set() {
    let html = StatusBadge::new("x").with_icon(true).build().render();
    assert!(!html.contains("<i"));

    let html = StatusBadge::new("x")
        .level(StatusLevel::Warning)
        .with_icon(true)
        .build()
        .render();
    assert!(html.contains("<i class=\"fas fa-exclamation-circle\""));
}

#[test]
fn every_status_badge_convenience_method_sets_its_own_level() {
    assert!(
        StatusBadge::new("x")
            .info()
            .build()
            .render()
            .contains("status-info")
    );
    assert!(
        StatusBadge::new("x")
            .success()
            .build()
            .render()
            .contains("status-success")
    );
    assert!(
        StatusBadge::new("x")
            .warning()
            .build()
            .render()
            .contains("status-warning")
    );
    assert!(
        StatusBadge::new("x")
            .error()
            .build()
            .render()
            .contains("status-error")
    );
}

#[test]
fn a_bare_alert_box_is_just_the_message_with_no_level_or_close_button() {
    let html = AlertBox::new("hello").build().render();
    assert_eq!(html, "<div class=\"alert\"><span>hello</span></div>");
}

#[test]
fn a_levelled_alert_box_carries_a_prefixed_level_class() {
    let html = AlertBox::new("msg")
        .level(StatusLevel::Error)
        .build()
        .render();
    assert!(html.contains("class=\"alert alert-status-error\""));
}

#[test]
fn a_closeable_alert_box_adds_a_close_button_with_accessible_attributes() {
    let html = AlertBox::new("msg").closeable(true).build().render();
    assert!(html.contains("class=\"alert-close\""));
    assert!(html.contains("role=\"button\""));
    assert!(html.contains("aria-label=\"Close alert\""));
    assert!(html.contains("class=\"fas fa-times\""));
}

#[test]
fn every_alert_box_convenience_method_sets_its_own_level() {
    let ab = AlertBox::new;
    assert!(
        ab("x")
            .info()
            .build()
            .render()
            .contains("alert-status-info")
    );
    assert!(
        ab("x")
            .success()
            .build()
            .render()
            .contains("alert-status-success")
    );
    assert!(
        ab("x")
            .warning()
            .build()
            .render()
            .contains("alert-status-warning")
    );
    assert!(
        ab("x")
            .error()
            .build()
            .render()
            .contains("alert-status-error")
    );
}

#[test]
fn success_warning_and_error_badge_helpers_include_the_icon() {
    let html = success_badge("ok").render();
    assert!(html.contains("status-success"));
    assert!(html.contains("<i class=\"fas fa-check-circle\""));

    let html = warning_badge("careful").render();
    assert!(html.contains("status-warning"));
    assert!(html.contains("<i class=\"fas fa-exclamation-circle\""));

    let html = error_badge("bad").render();
    assert!(html.contains("status-error"));
    assert!(html.contains("<i class=\"fas fa-times-circle\""));
}

#[test]
fn info_success_warning_and_error_alert_helpers_set_their_level_and_are_not_closeable() {
    let html = info_alert("info msg").render();
    assert!(html.contains("alert-status-info"));
    assert!(!html.contains("alert-close"));

    assert!(success_alert("s").render().contains("alert-status-success"));
    assert!(warning_alert("w").render().contains("alert-status-warning"));
    assert!(error_alert("e").render().contains("alert-status-error"));
}
