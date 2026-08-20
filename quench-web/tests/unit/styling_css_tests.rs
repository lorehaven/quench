//! Unit tests for `styling/css.rs`.

use quench_web::styling::css::CssRule;

#[test]
fn a_rule_with_no_properties_or_children_is_an_empty_block() {
    assert_eq!(CssRule::new(".empty").render(), ".empty {\n}\n");
}

#[test]
fn properties_render_in_declaration_order() {
    let rendered = CssRule::new(".box")
        .property("color", "red")
        .property("display", "flex")
        .render();
    assert_eq!(rendered, ".box {\n    color: red;\n    display: flex;\n}\n");
}

#[test]
fn a_child_rule_is_indented_one_level_deeper() {
    let rendered = CssRule::new(".parent")
        .property("display", "flex")
        .child(CssRule::new("&:hover").property("color", "blue"))
        .render();
    assert_eq!(
        rendered,
        ".parent {\n    display: flex;\n    &:hover {\n        color: blue;\n    }\n}\n"
    );
}

#[test]
fn grandchildren_indent_two_levels_deep() {
    let rendered = CssRule::new("a")
        .child(CssRule::new("b").child(CssRule::new("c").property("k", "v")))
        .render();
    assert_eq!(
        rendered,
        "a {\n    b {\n        c {\n            k: v;\n        }\n    }\n}\n"
    );
}
