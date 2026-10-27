use super::ThemeSpec;
use super::palette::Palette;
use crate::css::CssRule;

#[derive(Debug)]
pub struct BootstrapDarkTheme;

impl BootstrapDarkTheme {
    /// The colour tokens of this theme.
    pub const PALETTE: Palette = Palette::new(
        "#ffda6a",
        ["#75b798", "#5f9f84", "#4f8a72", "#3d6f5b", "#2b4f41"],
        "#ea868f",
        "#495057",
        [
            "#f8f9fa", "#f1f3f5", "#e9ecef", "#dee2e6", "#ced4da", "#adb5bd", "#6c757d", "#495057",
            "#343a40", "#212529", "#0f1114",
        ],
    );
}

impl ThemeSpec for BootstrapDarkTheme {
    fn colors() -> Vec<CssRule> {
        Self::PALETTE.rules()
    }
}
