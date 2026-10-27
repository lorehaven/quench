//! Escaping and the attribute checks, with the vectors an attacker would try.

use quench_ui::html::{
    BLOCKED_URL, escape_attr, escape_raw_text, escape_text, is_valid_attr_name, is_valid_tag,
    is_void, sanitize_attr_value,
};
use quench_ui::prelude::*;
use quench_ui::render_to_string;

#[quench_test::test]
fn text_is_escaped_between_tags() {
    assert_eq!(
        escape_text("a & b < c > d \" '"),
        "a &amp; b &lt; c &gt; d \" '"
    );
    assert_eq!(
        render_to_string(p().text("<script>alert(1)</script>")),
        "<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>"
    );
    assert_eq!(
        render_to_string("</p><img src=x onerror=alert(1)>"),
        "&lt;/p&gt;&lt;img src=x onerror=alert(1)&gt;"
    );
}

#[quench_test::test]
fn attribute_values_cannot_break_out_of_their_quotes() {
    assert_eq!(
        escape_attr("\"' <>&\0"),
        "&quot;&#39; &lt;&gt;&amp;\u{fffd}"
    );
    let html = render_to_string(div().attr("title", "\" onmouseover=\"alert(1)"));
    assert_eq!(
        html,
        "<div title=\"&quot; onmouseover=&quot;alert(1)\"></div>"
    );
    let html = render_to_string(div().attr_fn("data-x", || "'><script>".to_string()));
    assert_eq!(html, "<div data-x=\"&#39;&gt;&lt;script&gt;\"></div>");
}

#[quench_test::test]
fn event_handler_attributes_and_odd_names_are_dropped() {
    let html = render_to_string(
        div()
            .attr("onclick", "alert(1)")
            .attr("OnLoad", "alert(1)")
            .attr("on", "x")
            .attr("bad name", "x")
            .attr("a\"b", "x")
            .attr("", "x")
            .attr("x><y", "x")
            .attr("data-ok", "1")
            .attr_fn("onmouseover", || "alert(1)".into()),
    );
    assert_eq!(html, "<div data-ok=\"1\"></div>");
    for ok in ["id", "data-x", "aria-label", "xlink:href", "x_y", "a.b"] {
        assert!(is_valid_attr_name(ok), "{ok}");
    }
    for bad in ["", "onclick", "ONCLICK", "a b", "a=b", "a\"", "a>"] {
        assert!(!is_valid_attr_name(bad), "{bad:?}");
    }
}

#[quench_test::test]
fn script_urls_are_blocked_however_they_are_spelled() {
    for url in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "  javascript:alert(1)",
        "java\tscript:alert(1)",
        "java\nscript:alert(1)",
        "\u{1}javascript:alert(1)",
        "vbscript:msgbox(1)",
        "data:text/html,<script>alert(1)</script>",
    ] {
        assert_eq!(sanitize_attr_value("href", url), BLOCKED_URL, "{url:?}");
        let html = render_to_string(a().attr("href", url));
        assert!(html.contains(BLOCKED_URL), "{html}");
    }
    for url in [
        "/path?x=1",
        "https://example.com",
        "mailto:a@b.c",
        "#frag",
        "data:image/png;base64,AAAA",
        "javascript-guide.html",
    ] {
        assert_eq!(sanitize_attr_value("href", url), url, "{url}");
    }
    // Only URL attributes are looked at.
    assert_eq!(
        sanitize_attr_value("title", "javascript:alert(1)"),
        "javascript:alert(1)"
    );
    assert_eq!(sanitize_attr_value("SRC", "javascript:x"), BLOCKED_URL);
    // A reactive URL is checked on every update.
    let url = create_signal("/ok".to_string());
    let u = url.clone();
    let r = std::rc::Rc::new(TestRenderer::new());
    let root = r.root();
    let m = mount(&r, &root, a().attr_fn("href", move || u.get()));
    url.set("javascript:alert(1)".into());
    assert_eq!(
        r.attr(r.find_by_tag("a")[0], "href").as_deref(),
        Some(BLOCKED_URL)
    );
    m.unmount();
}

#[quench_test::test]
fn tag_names_are_validated() {
    for ok in ["div", "h1", "my-element", "DIV"] {
        assert!(is_valid_tag(ok));
    }
    for bad in ["", "1a", "a b", "a>", "a/", "-a", "a_b"] {
        assert!(!is_valid_tag(bad), "{bad:?}");
    }
    assert!(is_void("input") && is_void("br") && !is_void("div"));
}

#[quench_test::test]
fn script_and_style_text_cannot_close_their_element() {
    assert_eq!(
        escape_raw_text("a </script> b <!-- c"),
        "a <\\/script> b \\x3c!-- c"
    );
    assert_eq!(escape_raw_text("x < y"), "x < y");
    let html = render_to_string(element("script").text("var s = '</script><img onerror=x>';"));
    assert_eq!(
        html,
        "<script>var s = '<\\/script><img onerror=x>';</script>"
    );
    let html = render_to_string(element("style").text("a{}</style>"));
    assert_eq!(html, "<style>a{}<\\/style></style>");
}

#[quench_test::test]
fn void_elements_have_no_content_or_closing_tag() {
    assert_eq!(
        render_to_string(
            div()
                .child(input().attr("type", "text").child("ignored"))
                .child(img().attr("alt", "x"))
        ),
        "<div><input type=\"text\"><img alt=\"x\"></div>"
    );
}

#[quench_test::test]
fn reactive_content_is_escaped_on_every_update() {
    let s = create_signal("<b>".to_string());
    let s2 = s.clone();
    let r = std::rc::Rc::new(TestRenderer::new());
    let root = r.root();
    let m = mount(&r, &root, p().text_fn(move || s2.get()));
    assert_eq!(r.html(), "<p>&lt;b&gt;</p>");
    s.set("</p><script>".into());
    assert_eq!(r.html(), "<p>&lt;/p&gt;&lt;script&gt;</p>");
    m.unmount();
}

#[quench_test::test]
fn render_to_string_writes_regions_with_anchor_comments_and_leaves_nothing_running() {
    let n = create_signal(1);
    let n2 = n.clone();
    let html =
        render_to_string(div().child(View::dynamic(move || n2.get().to_string().into_view())));
    assert_eq!(html, "<div>1<!----></div>");
    n.set(2); // the discarded view's effect is gone
}

#[quench_test::test]
fn style_properties_with_odd_names_are_dropped_and_values_are_escaped() {
    let html = render_to_string(
        div()
            .style("color", "red")
            .style("bad prop", "x")
            .style("x:y", "z")
            .style("width", "\"><b>"),
    );
    assert_eq!(
        html,
        "<div style=\"color: red; width: &quot;&gt;&lt;b&gt;\"></div>"
    );
}

#[quench_test::test]
fn serialising_an_inspected_node_returns_just_that_subtree() {
    let r = std::rc::Rc::new(TestRenderer::new());
    let root = r.root();
    let m = mount(
        &r,
        &root,
        div().child(span().text("a")).child(p().text("b")),
    );
    assert_eq!(r.outer_html(r.find_by_tag("p")[0]), "<p>b</p>");
    assert_eq!(r.find_by_attr("id", "none"), Vec::new());
    assert_eq!(r.tag(r.find_by_tag("p")[0]).as_deref(), Some("p"));
    assert_eq!(r.tag(r.children(r.find_by_tag("p")[0])[0]), None);
    assert_eq!(r.attr(r.children(r.find_by_tag("p")[0])[0], "x"), None);
    assert_eq!(
        r.parent(r.find_by_tag("p")[0]),
        Some(r.find_by_tag("div")[0])
    );
    assert!(format!("{r:?}").contains("TestRenderer"));
    let _ = TestRenderer::default();
    m.unmount();
}
