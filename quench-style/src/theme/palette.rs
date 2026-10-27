use crate::css::CssRule;
use std::fmt::{Display, Formatter};

/// An sRGB colour, written as `#rrggbb`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Color(u32);

impl Color {
    /// From `0xRRGGBB`.
    #[must_use]
    pub const fn rgb(value: u32) -> Self {
        Self(value & 0x00ff_ffff)
    }

    /// From `#rrggbb`. Evaluated at compile time for constants, so a typo fails the build.
    ///
    /// # Panics
    /// If `value` is not `#` followed by six hex digits.
    #[must_use]
    pub const fn hex(value: &str) -> Self {
        let bytes = value.as_bytes();
        assert!(bytes.len() == 7 && bytes[0] == b'#', "expected #rrggbb");
        let mut out = 0u32;
        let mut i = 1;
        while i < 7 {
            let digit = match bytes[i] {
                b @ b'0'..=b'9' => b - b'0',
                b @ b'a'..=b'f' => b - b'a' + 10,
                b @ b'A'..=b'F' => b - b'A' + 10,
                _ => panic!("expected hex digits in #rrggbb"),
            };
            out = (out << 4) | digit as u32;
            i += 1;
        }
        Self(out)
    }

    #[must_use]
    pub const fn red(self) -> u8 {
        ((self.0 >> 16) & 0xff) as u8
    }

    #[must_use]
    pub const fn green(self) -> u8 {
        ((self.0 >> 8) & 0xff) as u8
    }

    #[must_use]
    pub const fn blue(self) -> u8 {
        (self.0 & 0xff) as u8
    }
}

impl Display for Color {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:06x}", self.0)
    }
}

/// The gray steps every palette defines, lightest name first.
pub const GRAY_STEPS: [u16; 11] = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];

/// The success steps every palette defines.
pub const SUCCESS_STEPS: [u16; 5] = [500, 600, 700, 800, 900];

/// The colour tokens of a theme, published as `--bs-*` custom properties.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Palette {
    pub warning: Color,
    /// Indexed like [`SUCCESS_STEPS`].
    pub success: [Color; 5],
    pub danger: Color,
    pub code_bg: Color,
    /// Indexed like [`GRAY_STEPS`].
    pub gray: [Color; 11],
}

impl Palette {
    /// Every token as `(custom property, colour)`, in the order they are emitted.
    #[must_use]
    pub fn tokens(&self) -> Vec<(String, Color)> {
        let mut out = vec![("--bs-warning".to_string(), self.warning)];
        out.extend(
            SUCCESS_STEPS
                .iter()
                .zip(self.success)
                .map(|(step, color)| (format!("--bs-success-{step}"), color)),
        );
        out.push(("--bs-danger".to_string(), self.danger));
        out.push(("--bs-code-bg".to_string(), self.code_bg));
        out.extend(
            GRAY_STEPS
                .iter()
                .zip(self.gray)
                .map(|(step, color)| (format!("--bs-gray-{step}"), color)),
        );
        out
    }

    /// The theme stylesheet: the `:root` tokens and the status colour helpers.
    #[must_use]
    pub fn rules(&self) -> Vec<CssRule> {
        let root = self
            .tokens()
            .into_iter()
            .fold(CssRule::new(":root"), |rule, (name, color)| {
                rule.property(&name, &color.to_string())
            });
        vec![
            root,
            CssRule::new(".color-green").property("color", "var(--bs-success-700)"),
            CssRule::new(".color-yellow").property("color", "var(--bs-warning)"),
            CssRule::new(".color-red").property("color", "var(--bs-danger)"),
        ]
    }
}

const fn colors<const N: usize>(values: [&str; N]) -> [Color; N] {
    let mut out = [Color::rgb(0); N];
    let mut i = 0;
    while i < N {
        out[i] = Color::hex(values[i]);
        i += 1;
    }
    out
}

impl Palette {
    /// Builds a palette from `#rrggbb` strings.
    ///
    /// # Panics
    /// If a value is not `#rrggbb` (a compile error in a `const`).
    #[must_use]
    pub const fn new(
        warning: &str,
        success: [&str; 5],
        danger: &str,
        code_bg: &str,
        gray: [&str; 11],
    ) -> Self {
        Self {
            warning: Color::hex(warning),
            success: colors(success),
            danger: Color::hex(danger),
            code_bg: Color::hex(code_bg),
            gray: colors(gray),
        }
    }
}
