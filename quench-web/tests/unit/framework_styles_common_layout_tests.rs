//! Unit tests for `framework/styles/common/layout.rs`.

use quench_web::framework::styles::common::{content, footer, header};

#[test]
fn header_is_one_rule_with_a_nested_nav_trigger() {
    let rules = header();
    assert_eq!(rules.len(), 1);
    let rendered = rules[0].render();
    assert!(rendered.starts_with("header,\n.q-shell-header {"));
    assert!(rendered.contains("nav,\n.q-shell-nav-trigger {"));
    assert!(rendered.contains("&:hover {"));
    assert!(rendered.contains("&:active {"));
}

#[test]
fn content_is_the_scroll_area_and_its_inner_wrapper() {
    let rules = content();
    assert_eq!(rules.len(), 2);
    assert!(
        rules[0]
            .render()
            .starts_with(".content,\n.q-shell-content {")
    );
    assert!(
        rules[1]
            .render()
            .starts_with(".content-inner,\n.q-shell-content-inner {")
    );
}

#[test]
fn footer_is_one_rule() {
    let rules = footer();
    assert_eq!(rules.len(), 1);
    assert!(rules[0].render().starts_with("footer,\n.q-shell-footer {"));
}
