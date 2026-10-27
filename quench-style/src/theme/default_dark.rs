use super::ThemeSpec;
use super::palette::Palette;
use crate::css::CssRule;

#[derive(Debug)]
pub struct DefaultDarkTheme;

impl DefaultDarkTheme {
    /// The colour tokens of this theme.
    pub const PALETTE: Palette = Palette::new(
        "#f59e0b",
        ["#10b981", "#059669", "#047857", "#065f46", "#064e3b"],
        "#be123c",
        "#1f2937",
        [
            "#fafafa", "#f5f5f5", "#e5e5e5", "#d4d4d4", "#a3a3a3", "#737373", "#525252", "#262626",
            "#121212", "#0d0d0d", "#050505",
        ],
    );
}

impl ThemeSpec for DefaultDarkTheme {
    fn colors() -> Vec<CssRule> {
        Self::PALETTE.rules()
    }
}
