//! Design tokens: the non-colour scales (spacing, radii, type, elevation, motion, targets).
//!
//! Each token is data first. [`stylesheet`] publishes them as `--q-*` custom properties for CSS
//! front ends, and the same constants are available to Rust for a renderer that does not use CSS.
//! Colours are in [`crate::theme::Palette`] because they differ per theme.

use crate::css::CssRule;
use std::fmt::{Display, Formatter};

/// A named design value.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Token<T> {
    /// The CSS custom property, `--q-...`.
    pub name: &'static str,
    pub value: T,
}

impl<T: Display> Token<T> {
    /// The value as it appears in CSS.
    #[must_use]
    pub fn css(&self) -> String {
        self.value.to_string()
    }
}

/// A CSS length.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Length {
    Rem(f32),
    Px(f32),
}

impl Length {
    /// The length in CSS pixels, with `1rem = 16px` as in every browser default.
    #[must_use]
    pub const fn px(self) -> f32 {
        match self {
            Self::Rem(v) => v * 16.0,
            Self::Px(v) => v,
        }
    }
}

impl Display for Length {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rem(v) => write!(f, "{v}rem"),
            Self::Px(v) => write!(f, "{v}px"),
        }
    }
}

/// A unitless number such as a line height.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Number(pub f32);

impl Display for Number {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A duration in milliseconds.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Millis(pub u32);

impl Display for Millis {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

/// The viewport width, in CSS pixels, at or below which layouts switch to the narrow form.
/// The shared stylesheet uses the same value in its `max-width` queries.
pub const NARROW_VIEWPORT_PX: u32 = 768;

/// Spacing scale on a 4px grid.
pub const SPACING: [Token<Length>; 8] = [
    Token {
        name: "--q-space-1",
        value: Length::Rem(0.25),
    },
    Token {
        name: "--q-space-2",
        value: Length::Rem(0.5),
    },
    Token {
        name: "--q-space-3",
        value: Length::Rem(0.75),
    },
    Token {
        name: "--q-space-4",
        value: Length::Rem(1.0),
    },
    Token {
        name: "--q-space-5",
        value: Length::Rem(1.5),
    },
    Token {
        name: "--q-space-6",
        value: Length::Rem(2.0),
    },
    Token {
        name: "--q-space-7",
        value: Length::Rem(3.0),
    },
    Token {
        name: "--q-space-8",
        value: Length::Rem(4.0),
    },
];

/// Corner radii. `md` is the radius the shell panels already use.
pub const RADII: [Token<Length>; 4] = [
    Token {
        name: "--q-radius-sm",
        value: Length::Rem(0.2),
    },
    Token {
        name: "--q-radius-md",
        value: Length::Rem(0.35),
    },
    Token {
        name: "--q-radius-lg",
        value: Length::Rem(0.75),
    },
    Token {
        name: "--q-radius-full",
        value: Length::Px(9999.0),
    },
];

/// Type scale; `3xl` and `2xl` match the shared `h1` and `h2` sizes.
pub const TEXT_SIZES: [Token<Length>; 7] = [
    Token {
        name: "--q-text-xs",
        value: Length::Rem(0.75),
    },
    Token {
        name: "--q-text-sm",
        value: Length::Rem(0.875),
    },
    Token {
        name: "--q-text-md",
        value: Length::Rem(1.0),
    },
    Token {
        name: "--q-text-lg",
        value: Length::Rem(1.25),
    },
    Token {
        name: "--q-text-xl",
        value: Length::Rem(1.5),
    },
    Token {
        name: "--q-text-2xl",
        value: Length::Rem(2.0),
    },
    Token {
        name: "--q-text-3xl",
        value: Length::Rem(3.0),
    },
];

/// Line heights.
pub const LINE_HEIGHTS: [Token<Number>; 2] = [
    Token {
        name: "--q-leading-tight",
        value: Number(1.2),
    },
    Token {
        name: "--q-leading-normal",
        value: Number(1.5),
    },
];

/// Elevation as box shadows, from flat to a floating sheet.
pub const ELEVATION: [Token<&str>; 4] = [
    Token {
        name: "--q-elevation-0",
        value: "none",
    },
    Token {
        name: "--q-elevation-1",
        value: "0 1px 2px rgba(0, 0, 0, 0.3)",
    },
    Token {
        name: "--q-elevation-2",
        value: "0 4px 8px rgba(0, 0, 0, 0.35)",
    },
    Token {
        name: "--q-elevation-3",
        value: "0 12px 24px rgba(0, 0, 0, 0.4)",
    },
];

/// Animation durations. They collapse to near zero under `prefers-reduced-motion`.
pub const DURATIONS: [Token<Millis>; 3] = [
    Token {
        name: "--q-dur-fast",
        value: Millis(120),
    },
    Token {
        name: "--q-dur-base",
        value: Millis(200),
    },
    Token {
        name: "--q-dur-slow",
        value: Millis(320),
    },
];

/// The duration that stands in for zero: some engines skip `transitionend` at exactly `0s`.
pub const REDUCED_MOTION: Millis = Millis(1);

/// Easing curves.
pub const EASING: [Token<&str>; 2] = [
    Token {
        name: "--q-ease-standard",
        value: "cubic-bezier(0.2, 0, 0, 1)",
    },
    Token {
        name: "--q-ease-exit",
        value: "cubic-bezier(0.4, 0, 1, 1)",
    },
];

/// Minimum interactive size for touch input (44 CSS pixels).
pub const TOUCH_TARGET: Token<Length> = Token {
    name: "--q-touch-target",
    value: Length::Px(44.0),
};

/// Minimum interactive size for a fine pointer (32 CSS pixels).
pub const POINTER_TARGET: Token<Length> = Token {
    name: "--q-pointer-target",
    value: Length::Px(32.0),
};

/// Every token as `(custom property, CSS value)`, in emission order.
#[must_use]
pub fn all() -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let mut push = |name: &'static str, value: String| out.push((name, value));
    for t in SPACING.iter().chain(&RADII).chain(&TEXT_SIZES) {
        push(t.name, t.css());
    }
    for t in &LINE_HEIGHTS {
        push(t.name, t.css());
    }
    for t in ELEVATION.iter().chain(&EASING) {
        push(t.name, t.value.to_string());
    }
    for t in &DURATIONS {
        push(t.name, t.css());
    }
    push(TOUCH_TARGET.name, TOUCH_TARGET.css());
    push(POINTER_TARGET.name, POINTER_TARGET.css());
    out
}

/// The `:root` rule with every token, plus the reduced-motion override.
#[must_use]
pub fn rules() -> Vec<CssRule> {
    let root = all()
        .into_iter()
        .fold(CssRule::new(":root"), |rule, (name, value)| {
            rule.property(name, &value)
        });
    let reduced = DURATIONS.iter().fold(CssRule::new(":root"), |rule, t| {
        rule.property(t.name, &REDUCED_MOTION.to_string())
    });
    vec![
        root,
        CssRule::new("@media (prefers-reduced-motion: reduce)").child(reduced),
    ]
}

/// The tokens as a stylesheet. Opt-in: the shared stylesheet does not include it.
#[must_use]
pub fn stylesheet() -> String {
    rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n")
}
