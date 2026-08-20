//! Unit tests for `framework/styles/common/modal.rs`.

use quench_web::framework::styles::common::modal;
use quench_web::styling::css::CssRule;

#[test]
fn modal_returns_the_overlay_and_three_layouts() {
    let rules = modal();
    assert_eq!(rules.len(), 4);
    let rendered = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains(".modal-overlay {"));
    assert!(rendered.contains(".modal-side,\n.modal-center,\n.q-shell-modal-side {"));
    assert!(rendered.contains(".modal-side {"));
    assert!(rendered.contains(".modal-center {"));
    // The center layout's action buttons, nested three levels deep.
    assert!(rendered.contains(".buttons {"));
    assert!(rendered.contains("button {"));
}
