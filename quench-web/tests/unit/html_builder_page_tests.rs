//! Unit tests for `html/builder/page.rs`.

use quench_web::html::builder::elements::{div, p, span};
use quench_web::html::builder::page::{Link, PageBuilder, Script, pretty_print_html};

#[test]
fn link_carries_its_rel_href_and_extra_attrs() {
    let link = Link::new("stylesheet", "/app.css").attr("media", "screen");
    assert_eq!(link.rel, "stylesheet");
    assert_eq!(link.href, "/app.css");
    assert_eq!(link.attrs.get("media"), Some(&"screen".to_string()));
}

#[test]
fn a_linked_script_defers_by_default() {
    let script = Script::new("/app.js");
    assert!(!script.is_inline());
    assert_eq!(script.render(), "<script src=\"/app.js\" defer></script>");
}

#[test]
fn immediate_removes_the_default_defer() {
    let script = Script::new("/app.js").immediate();
    assert_eq!(script.render(), "<script src=\"/app.js\"></script>");
}

#[test]
fn crossorigin_is_included_when_set() {
    let script = Script::new("/app.js").immediate().crossorigin("anonymous");
    assert_eq!(
        script.render(),
        "<script src=\"/app.js\" crossorigin=\"anonymous\"></script>"
    );
}

#[test]
fn an_inline_script_does_not_defer_by_default_and_reports_itself_as_inline() {
    let script = Script::inline("console.log(1)");
    assert!(script.is_inline());
    assert_eq!(script.render(), "<script>console.log(1)</script>");
}

#[test]
fn defer_can_be_re_enabled_on_an_inline_script() {
    let script = Script::inline("console.log(1)").defer();
    assert_eq!(script.render(), "<script defer>console.log(1)</script>");
}

#[test]
fn script_converts_from_owned_and_borrowed_strings() {
    let owned: Script = "a".to_string().into();
    let borrowed: Script = "b".into();
    let by_ref: Script = (&"c".to_string()).into();
    assert_eq!(owned.render(), "<script>a</script>");
    assert_eq!(borrowed.render(), "<script>b</script>");
    assert_eq!(by_ref.render(), "<script>c</script>");
}

#[test]
fn the_js_macro_builds_an_inline_script_with_format_arguments() {
    let name = "world";
    let script = quench_web::js!("console.log('hello {}')", name);
    assert_eq!(
        script.render(),
        "<script>console.log('hello world')</script>"
    );
}

#[test]
fn page_builder_assembles_title_links_scripts_and_content() {
    let html = PageBuilder::new()
        .title("My Page")
        .links(vec![Link::new("stylesheet", "/app.css")])
        .scripts(vec![Script::new("/app.js").immediate()])
        .content(div().attr("id", "root").text("hello"))
        .build();

    assert!(html.contains("<title>\n            My Page\n        </title>"));
    assert!(html.contains("<link href=\"/app.css\" rel=\"stylesheet\">"));
    assert!(html.contains("<script src=\"/app.js\"></script>"));
    assert!(html.contains("id=\"root\""));
    assert!(html.contains("hello"));
}

#[test]
fn head_link_static_attrs_are_merged_onto_every_link() {
    let html = PageBuilder::new()
        .title("t")
        .links(vec![Link::new("preload", "/font.woff2")])
        .head_link_static_attr("crossorigin", "anonymous")
        .content(div())
        .build();

    assert!(html.contains("crossorigin=\"anonymous\""));
    assert!(html.contains("rel=\"preload\""));
}

#[test]
#[should_panic]
fn build_panics_without_content() {
    PageBuilder::new().title("t").build();
}

#[test]
fn pretty_print_html_indents_element_tags() {
    // html5ever always parses a full document: a bare fragment gets an
    // implied <html><head></head><body>...</body></html> wrapped around it.
    let pretty = pretty_print_html("<div><span>hi</span></div>");
    assert_eq!(
        pretty,
        "<html>\n    <head>\n    </head>\n    <body>\n        <div>\n            <span>\n                hi\n            </span>\n        </div>\n    </body>\n</html>\n"
    );
}

#[test]
fn pretty_print_html_preserves_preformatted_tag_content_verbatim() {
    // `pre`/`script`/`style`/`code`/`textarea` must not have their inner
    // whitespace reformatted - that would change what they mean.
    let pretty = pretty_print_html("<pre>  a\n   b  </pre>");
    assert!(pretty.contains("<pre>  a\n   b  </pre>"));
}

#[test]
fn pretty_print_html_escapes_attribute_values() {
    let pretty = pretty_print_html("<div title=\"a&b\"></div>");
    assert!(pretty.contains("title=\"a&amp;b\""));
}

// -- text is escaped on the way out ---------------------------------
//
// The parser decodes entities on the way in, so a serializer that writes text
// nodes back verbatim turns any displayed text into live markup.

#[test]
fn pretty_print_html_escapes_text_nodes() {
    let pretty = pretty_print_html("<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>");
    assert!(
        pretty.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{pretty}"
    );
    assert!(!pretty.contains("<script>"), "{pretty}");
}

#[test]
fn pretty_print_html_keeps_ampersands_and_quotes_valid_in_text() {
    let pretty = pretty_print_html("<p>a &amp; b &quot;c&quot; &lt;d&gt;</p>");
    assert!(
        pretty.contains("a &amp; b &quot;c&quot; &lt;d&gt;"),
        "{pretty}"
    );
}

#[test]
fn pretty_print_html_does_not_double_escape_on_a_second_pass() {
    let once = pretty_print_html("<p>&lt;b&gt; &amp; more</p>");
    let twice = pretty_print_html(&once);
    assert_eq!(once, twice);
}

#[test]
fn pretty_print_html_still_writes_script_and_style_bodies_verbatim() {
    let pretty = pretty_print_html(
        "<script>if (a < b && c > d) { x = \"y\"; }</script><style>a > b { color: red }</style>",
    );
    assert!(
        pretty.contains("if (a < b && c > d) { x = \"y\"; }"),
        "{pretty}"
    );
    assert!(pretty.contains("a > b { color: red }"), "{pretty}");
}

#[test]
fn pretty_print_html_escapes_the_text_inside_pre_code_and_textarea() {
    for tag in ["pre", "code", "textarea"] {
        let pretty = pretty_print_html(&format!(
            "<{tag}>&lt;img src=x onerror=alert(1)&gt;</{tag}>"
        ));
        assert!(
            pretty.contains("&lt;img src=x onerror=alert(1)&gt;"),
            "{tag}: {pretty}"
        );
        assert!(!pretty.contains("<img"), "{tag}: {pretty}");
    }
}

#[test]
fn pretty_print_html_keeps_the_whitespace_inside_pre_while_escaping() {
    let pretty = pretty_print_html("<pre>  a &lt; b\n   c  </pre>");
    assert!(pretty.contains("<pre>  a &lt; b\n   c  </pre>"), "{pretty}");
}

#[test]
fn text_set_on_an_element_reaches_the_page_escaped() {
    let hostile = "<img src=x onerror=alert(document.cookie)>";
    let html = PageBuilder::new()
        .title("t")
        .content(div().child(span().text(hostile)).child(p().text("a & b")))
        .build();
    assert!(!html.contains("<img"), "{html}");
    assert!(
        html.contains("&lt;img src=x onerror=alert(document.cookie)&gt;"),
        "{html}"
    );
    assert!(html.contains("a &amp; b"));
}

#[test]
fn raw_text_is_still_markup_for_the_callers_that_ask_for_it() {
    let html = PageBuilder::new()
        .title("t")
        .content(div().child(p().text("<b>bold</b>").raw()))
        .build();
    assert!(html.contains("<b>"), "{html}");
}

#[test]
fn an_attribute_and_text_holding_the_same_hostile_string_are_both_safe() {
    let hostile = "\"><script>alert(1)</script>";
    let html = PageBuilder::new()
        .title("t")
        .content(div().attr("title", hostile).text(hostile))
        .build();
    assert!(!html.contains("<script>alert"), "{html}");
}
