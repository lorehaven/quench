//! Unit tests for `html/builder/elements.rs`.

use quench_web::Element;
use quench_web::html::builder::elements::*;

type Constructor = (fn() -> Element, &'static str);

#[test]
fn every_bare_tag_constructor_renders_its_own_tag() {
    let constructors: &[Constructor] = &[
        (button, "button"),
        (div, "div"),
        (strong, "strong"),
        (header, "header"),
        (content, "content"),
        (footer, "footer"),
        (nav, "nav"),
        (a, "a"),
        (h1, "h1"),
        (h2, "h2"),
        (h3, "h3"),
        (p, "p"),
        (pre, "pre"),
        (span, "span"),
        (form, "form"),
        (label, "label"),
        (ul, "ul"),
        (li, "li"),
        (i, "i"),
        (input, "input"),
        (textarea, "textarea"),
        (select, "select"),
        (option, "option"),
        (meta, "meta"),
    ];

    for (constructor, tag) in constructors {
        assert_eq!(constructor().render(), format!("<{tag}></{tag}>"));
    }
}

#[test]
fn element_builds_an_arbitrary_tag_by_name() {
    assert_eq!(element("custom-tag").render(), "<custom-tag></custom-tag>");
}

#[test]
fn checkbox_is_an_input_of_type_checkbox() {
    assert_eq!(checkbox().render(), "<input type=\"checkbox\"></input>");
}

#[test]
fn style_wraps_its_content_in_a_style_tag_unescaped_by_the_caller() {
    // `style`/`script` use `Element::text`, which HTML-escapes - exactly
    // right for a text node, but worth pinning down since CSS/JS content
    // often contains characters `text` would otherwise mangle (like `>`).
    let html = style("a > b { color: red; }".to_string()).render();
    assert_eq!(html, "<style>a &gt; b { color: red; }</style>");
}

#[test]
fn script_wraps_its_content_in_a_script_tag() {
    let html = script("console.log('hi')".to_string()).render();
    assert_eq!(html, "<script>console.log(&#39;hi&#39;)</script>");
}
