use super::ThemeSpec;
use super::palette::Palette;
use crate::css::CssRule;

#[derive(Debug)]
pub struct DefaultLightTheme;

impl DefaultLightTheme {
    /// The colour tokens of this theme.
    pub const PALETTE: Palette = Palette::new(
        "#f59e0b",
        ["#10b981", "#059669", "#047857", "#065f46", "#064e3b"],
        "#be123c",
        "#1f2937",
        [
            "#0a0a0a", "#171717", "#262626", "#404040", "#525252", "#737373", "#a3a3a3", "#d4d4d4",
            "#e5e5e5", "#f5f5f5", "#fafafa",
        ],
    );
}

impl ThemeSpec for DefaultLightTheme {
    fn colors() -> Vec<CssRule> {
        Self::PALETTE.rules()
    }
}
