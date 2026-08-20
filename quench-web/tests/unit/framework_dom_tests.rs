//! Unit tests for `framework/dom/{events,modal,select}.rs`.

use quench_web::framework::dom::{
    on_dom_ready, set_select_value, toggle_modal, update_from_select,
};

#[test]
fn on_dom_ready_wraps_every_block_in_one_listener() {
    let js = on_dom_ready(&["a();".to_string(), "b();".to_string()]);
    assert_eq!(
        js,
        "document.addEventListener(\"DOMContentLoaded\", () => {\na();\nb();\n});"
    );
}

#[test]
fn toggle_modal_names_the_overlay_panel_and_class_it_toggles() {
    let js = toggle_modal("modal-overlay", "modal-side", "show");
    assert!(js.contains("getElementsByClassName('modal-overlay')"));
    assert!(js.contains("getElementsByClassName('modal-side')"));
    assert!(js.contains("classList.toggle('show')"));
}

#[test]
fn update_from_select_reads_the_selects_value_into_the_update_function() {
    let js = update_from_select("locale-select", "updateLocale");
    assert_eq!(
        js,
        "const selected = document.getElementById('locale-select').value;updateLocale(selected);"
    );
}

#[test]
fn set_select_value_guards_against_a_missing_select() {
    let js = set_select_value("theme-select", "getTheme");
    assert!(js.contains("document.getElementById('theme-select')"));
    assert!(js.contains("if (select) {"));
    assert!(js.contains("select.value = getTheme();"));
}
