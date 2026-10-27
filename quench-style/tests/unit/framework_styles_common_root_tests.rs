//! Unit tests for `framework/styles/common/root.rs`.

use quench_style::css::CssRule;
use quench_style::styles::common::root;

#[quench_test::test]
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
