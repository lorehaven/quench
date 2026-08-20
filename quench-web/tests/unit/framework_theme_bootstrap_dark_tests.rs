//! Unit tests for `framework/theme/bootstrap_dark.rs`.

use quench_web::framework::theme::ThemeSpec;
use quench_web::framework::theme::bootstrap_dark::BootstrapDarkTheme;
use quench_web::styling::css::CssRule;

#[test]
fn bootstrap_dark_declares_its_own_palette_and_three_color_utilities() {
    let rules = BootstrapDarkTheme::colors();
    assert_eq!(rules.len(), 4);
    let rendered = rules
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("--bs-danger: #ea868f;"));
    assert!(rendered.contains("--bs-gray-800: #343a40;"));
    assert!(rendered.contains(".color-green {"));
    assert!(rendered.contains(".color-yellow {"));
    assert!(rendered.contains(".color-red {"));
}
