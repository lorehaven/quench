//! Unit tests for `forms/mod.rs`.

use quench_web_components::forms::{FormInput, FormSelect, FormTextarea};

#[test]
fn a_bare_form_input_has_no_label_wrapper() {
    let html = FormInput::new("email").build().render();
    assert!(html.starts_with("<input"));
    assert!(html.contains("type=\"text\""));
    assert!(html.contains("name=\"email\""));
    assert!(html.contains("id=\"email\""));
    assert!(!html.contains("required"));
}

#[test]
fn a_labelled_form_input_wraps_the_label_and_input_together() {
    let html = FormInput::new("email")
        .label("Email address")
        .input_type("email")
        .placeholder("you@example.com")
        .value("preset@example.com")
        .required(true)
        .build()
        .render();

    assert!(html.starts_with("<div class=\"form-group\">"));
    assert!(html.contains("<label for=\"email\">Email address</label>"));
    assert!(html.contains("type=\"email\""));
    assert!(html.contains("placeholder=\"you@example.com\""));
    assert!(html.contains("value=\"preset@example.com\""));
    assert!(html.contains("required=\"required\""));
}

#[test]
fn a_bare_form_select_lists_its_options_with_none_selected() {
    let html = FormSelect::new("role")
        .option("admin", "Administrator")
        .option("user", "User")
        .build()
        .render();

    assert!(html.starts_with("<select"));
    assert!(html.contains("name=\"role\""));
    assert!(html.contains("<option value=\"admin\">Administrator</option>"));
    assert!(html.contains("<option value=\"user\">User</option>"));
    assert!(!html.contains("selected"));
}

#[test]
fn a_form_select_marks_the_selected_option_and_can_be_required_and_labelled() {
    let html = FormSelect::new("role")
        .label("Role")
        .options(vec![
            ("admin".to_string(), "Administrator".to_string()),
            ("user".to_string(), "User".to_string()),
        ])
        .selected("user")
        .required(true)
        .build()
        .render();

    assert!(html.starts_with("<div class=\"form-group\">"));
    assert!(html.contains("<label for=\"role\">Role</label>"));
    assert!(html.contains("required=\"required\""));
    // `Element`'s attributes are a HashMap, so `value`/`selected` can render
    // in either order - checked independently rather than as one substring.
    assert!(html.contains("<option") && html.contains("value=\"user\""));
    assert!(html.contains("selected=\"selected\""));
    assert!(html.contains(">User</option>"));
    // Only the selected option gets marked - `admin`'s tag has no `selected`.
    let admin_tag_start = html.find("value=\"admin\"").expect("admin option");
    let admin_tag_end = html[admin_tag_start..].find('>').unwrap() + admin_tag_start;
    assert!(!html[admin_tag_start..admin_tag_end].contains("selected"));
}

#[test]
fn a_bare_form_textarea_has_no_optional_attributes() {
    let html = FormTextarea::new("bio").build().render();
    assert!(html.starts_with("<textarea"));
    assert!(html.contains("name=\"bio\""));
    assert!(!html.contains("rows"));
    assert!(!html.contains("cols"));
}

#[test]
fn a_full_form_textarea_carries_every_optional_attribute_and_its_value_as_text() {
    let html = FormTextarea::new("bio")
        .label("Biography")
        .placeholder("Tell us about yourself")
        .rows(4)
        .cols(40)
        .required(true)
        .value("Existing bio")
        .build()
        .render();

    assert!(html.starts_with("<div class=\"form-group\">"));
    assert!(html.contains("<label for=\"bio\">Biography</label>"));
    assert!(html.contains("rows=\"4\""));
    assert!(html.contains("cols=\"40\""));
    assert!(html.contains("placeholder=\"Tell us about yourself\""));
    assert!(html.contains("required=\"required\""));
    assert!(html.contains(">Existing bio<"));
}
