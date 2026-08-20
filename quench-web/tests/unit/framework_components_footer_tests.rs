//! Unit tests for `framework/components/footer.rs`.

use quench_web::framework::components::footer::FooterBuilder;

#[test]
fn footer_builder_renders_a_labelled_paragraph_in_a_classed_footer() {
    let html = FooterBuilder::new().label("footer_label").build().render();
    assert!(html.starts_with("<footer"));
    assert!(html.contains("class=\"footer q-shell-footer\""));
    assert!(html.contains("<p data-i18n=\"footer_label\"></p>"));
}
