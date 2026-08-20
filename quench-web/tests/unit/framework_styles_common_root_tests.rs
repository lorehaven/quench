//! Unit tests for `framework/styles/common/root.rs`.

use quench_web::framework::styles::common::root;
use quench_web::styling::css::CssRule;

#[test]
fn root_declares_the_shell_custom_properties_and_base_rules() {
    let rules = root();
    assert_eq!(rules.len(), 5);
    let rendered = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains(":root {"));
    assert!(rendered.contains("--q-shell-font: 'Roboto', sans-serif;"));
    assert!(rendered.contains("html,\nbody {"));
    assert!(rendered.contains(".app,\n.q-shell-app {"));
    assert!(rendered.contains("&::-webkit-scrollbar-thumb:hover {"));
}
