use crate::css::CssRule;
use crate::styles::common::{content, elements, footer, header, modal, root};
use std::fmt::{Display, Formatter};

pub mod bootstrap_dark;
pub mod bootstrap_light;
pub mod default_dark;
pub mod default_light;
pub mod palette;

pub use palette::{Color, GRAY_STEPS, Palette, SUCCESS_STEPS};

pub trait ThemeSpec {
    fn colors() -> Vec<CssRule>;

    #[must_use]
    fn render() -> String {
        Self::colors()
            .into_iter()
            .map(|rule| rule.render())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Theme {
    DefaultDark,
    DefaultLight,
    BootstrapDark,
    BootstrapLight,
}

impl Display for Theme {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DefaultDark => write!(f, "default-dark"),
            Self::DefaultLight => write!(f, "default-light"),
            Self::BootstrapDark => write!(f, "bootstrap-dark"),
            Self::BootstrapLight => write!(f, "bootstrap-light"),
        }
    }
}

impl Theme {
    /// Every theme, in declaration order.
    pub const ALL: [Self; 4] = [
        Self::DefaultDark,
        Self::DefaultLight,
        Self::BootstrapDark,
        Self::BootstrapLight,
    ];

    pub fn iter() -> impl Iterator<Item = Self> {
        Self::ALL.into_iter()
    }

    #[must_use]
    pub fn theme(theme: Self) -> String {
        match theme {
            Self::DefaultDark => default_dark::DefaultDarkTheme::render(),
            Self::DefaultLight => default_light::DefaultLightTheme::render(),
            Self::BootstrapDark => bootstrap_dark::BootstrapDarkTheme::render(),
            Self::BootstrapLight => bootstrap_light::BootstrapLightTheme::render(),
        }
    }
}

impl Theme {
    /// The colour tokens of this theme.
    #[must_use]
    pub const fn palette(self) -> Palette {
        match self {
            Self::DefaultDark => default_dark::DefaultDarkTheme::PALETTE,
            Self::DefaultLight => default_light::DefaultLightTheme::PALETTE,
            Self::BootstrapDark => bootstrap_dark::BootstrapDarkTheme::PALETTE,
            Self::BootstrapLight => bootstrap_light::BootstrapLightTheme::PALETTE,
        }
    }
}

#[must_use]
pub fn theme_shared() -> String {
    vec![root(), header(), content(), footer(), elements(), modal()]
        .into_iter()
        .flatten()
        .map(|rule| rule.render())
        .collect::<Vec<_>>()
        .join("\n")
}
