//! Unit tests for `html/builder/page.rs`.

use quench_web::html::builder::elements::div;
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
