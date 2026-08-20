//! Unit tests for `framework/app/builder.rs`.

use quench_web::framework::app::AppBuilder;
use quench_web::html::builder::elements::div;

#[test]
fn build_assembles_a_full_page_with_the_active_theme_and_locale_scripts() {
    let html = AppBuilder::new()
        .title("Quench")
        .supported_locales(vec!["en-US".to_string()])
        .resources_prefix("/api".to_string())
        .page_content(div().attr("id", "root").text("hi"))
        .build();

    assert!(html.contains("Quench"));
    // The active theme's stylesheet is linked directly, not preloaded.
    assert!(html.contains("id=\"theme-link\""));
    assert!(html.contains("/api/assets/css/themes/default-dark.css"));
    // Every other theme is preloaded instead of linked.
    assert!(html.contains("rel=\"preload\""));
    assert!(html.contains("as=\"style\""));
    assert!(html.contains("/api/assets/js/translations.js"));
    assert!(html.contains("id=\"root\""));
}

#[test]
fn extra_links_and_scripts_are_appended_not_replaced() {
    let html = AppBuilder::new()
        .supported_locales(vec!["en-US".to_string()])
        .links(vec![quench_web::Link::new("canonical", "/page")])
        .scripts(vec![quench_web::Script::inline("console.log('extra')")])
        .page_content(div())
        .build();

    assert!(html.contains("rel=\"canonical\""));
    assert!(html.contains("console.log('extra')"));
    // The framework's own htmx script is still there alongside it.
    assert!(html.contains("htmx.org"));
}

#[test]
fn header_and_footer_are_optional() {
    let without = AppBuilder::new()
        .supported_locales(vec!["en-US".to_string()])
        .page_content(div())
        .build();
    assert!(!without.contains("<header"));
    assert!(!without.contains("<footer"));

    let with = AppBuilder::new()
        .supported_locales(vec!["en-US".to_string()])
        .header(quench_web::html::builder::elements::header())
        .footer(quench_web::html::builder::elements::footer())
        .page_content(div())
        .build();
    assert!(with.contains("<header"));
    assert!(with.contains("<footer"));
}
