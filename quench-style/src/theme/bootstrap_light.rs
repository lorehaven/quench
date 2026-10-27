use super::ThemeSpec;
use super::palette::Palette;
use crate::css::CssRule;

#[derive(Debug)]
pub struct BootstrapLightTheme;

impl BootstrapLightTheme {
    /// The colour tokens of this theme.
    pub const PALETTE: Palette = Palette::new(
        "#ffc107",
        ["#198754", "#157347", "#146c43", "#0f5132", "#0a3622"],
        "#dc3545",
        "#343a40",
        [
            "#0f1114", "#161a1d", "#1f252b", "#2b3035", "#343a40", "#495057", "#6c757d", "#adb5bd",
            "#ced4da", "#dee2e6", "#f8f9fa",
        ],
    );
}

impl ThemeSpec for BootstrapLightTheme {
    fn colors() -> Vec<CssRule> {
        Self::PALETTE.rules()
    }
}
