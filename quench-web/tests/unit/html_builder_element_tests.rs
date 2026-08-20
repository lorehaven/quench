//! Unit tests for `html/builder/element.rs`.
//!
//! `Element::new` is crate-private, so these go through the public tag
//! constructors in `html/builder/elements.rs` - exactly how a real caller
//! builds one.

use quench_web::html::builder::elements::{div, span};

#[test]
fn an_empty_element_renders_as_an_open_and_close_tag() {
    assert_eq!(div().render(), "<div></div>");
}

#[test]
fn attr_renders_as_a_quoted_attribute() {
    let html = div().attr("id", "main").render();
    assert_eq!(html, "<div id=\"main\"></div>");
}

#[test]
fn class_appends_rather_than_replaces_on_repeated_calls() {
    let html = div().class("a").class("b").render();
    assert_eq!(html, "<div class=\"a b\"></div>");
}

#[test]
fn text_is_html_escaped() {
    let html = div().text("<script>&\"'").render();
    assert_eq!(html, "<div>&lt;script&gt;&amp;&quot;&#39;</div>");
}

#[test]
fn raw_disables_escaping_of_the_text_content() {
    let html = div().raw().text("<b>bold</b>").render();
    assert_eq!(html, "<div><b>bold</b></div>");
}

#[test]
fn children_render_inside_the_parent_in_order() {
    let html = div()
        .child(span().text("a"))
        .child(span().text("b"))
        .render();
    assert_eq!(html, "<div><span>a</span><span>b</span></div>");
}

#[test]
fn child_opt_of_none_adds_nothing() {
    assert_eq!(div().child_opt(None).render(), "<div></div>");
}

#[test]
fn child_opt_of_some_behaves_like_child() {
    let html = div().child_opt(Some(span().text("x"))).render();
    assert_eq!(html, "<div><span>x</span></div>");
}

#[test]
fn on_click_and_on_change_render_as_escaped_inline_handlers() {
    let html = div().on_click("go()").render();
    assert_eq!(html, "<div onclick=\"go()\"></div>");

    let html = div().on_change("update()").render();
    assert_eq!(html, "<div onchange=\"update()\"></div>");
}

#[test]
fn defer_adds_a_bare_defer_attribute() {
    assert_eq!(div().defer().render(), "<div defer></div>");
}
