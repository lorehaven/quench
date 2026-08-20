//! Unit tests for `framework/theme/bootstrap_light.rs`.

use quench_web::framework::theme::ThemeSpec;
use quench_web::framework::theme::bootstrap_light::BootstrapLightTheme;
use quench_web::styling::css::CssRule;

#[test]
fn bootstrap_light_declares_its_own_palette_and_three_color_utilities() {
    let rules = BootstrapLightTheme::colors();
    assert_eq!(rules.len(), 4);
    let rendered = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("--bs-danger: #dc3545;"));
    assert!(rendered.contains("--bs-gray-800: #ced4da;"));
    assert!(rendered.contains(".color-green {"));
    assert!(rendered.contains(".color-yellow {"));
    assert!(rendered.contains(".color-red {"));
}
