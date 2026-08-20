//! Unit tests for `framework/styles/common/elements.rs`.

use quench_web::framework::styles::common::elements;
use quench_web::styling::css::CssRule;

#[test]
fn elements_returns_every_shared_widget_rule_exactly_once() {
    let rules = elements();
    assert_eq!(rules.len(), 17);

    let rendered = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");

    for selector in [
        "h1 {",
        "h2 {",
        "h3 {",
        "form {",
        "input {",
        ".password-wrapper {",
        "a.button,\nbutton {",
        "select {",
        "section {",
        ".separator {",
        ".separator-or {",
        ".loader {",
        ".tabs {",
        ".slides-container {",
        ".binding-code-input {",
        ".table {",
        ".table-mobile {",
    ] {
        assert!(
            rendered.contains(selector),
            "expected the rendered elements to contain `{selector}`"
        );
    }
}
