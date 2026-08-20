//! Unit tests for `framework/theme/default_light.rs`.

use quench_web::framework::theme::ThemeSpec;
use quench_web::framework::theme::default_light::DefaultLightTheme;
use quench_web::styling::css::CssRule;

#[test]
fn default_light_declares_its_own_palette_and_three_color_utilities() {
    let rules = DefaultLightTheme::colors();
    assert_eq!(rules.len(), 4);
    let rendered = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("--bs-danger: #be123c;"));
    assert!(rendered.contains("--bs-gray-800: #e5e5e5;"));
    assert!(rendered.contains(".color-green {"));
    assert!(rendered.contains(".color-yellow {"));
    assert!(rendered.contains(".color-red {"));
}
