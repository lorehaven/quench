/// A colour with straight (not premultiplied) alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The same colour with its alpha multiplied by `opacity` (0 to 1).
    #[must_use]
    pub fn with_opacity(self, opacity: f32) -> Self {
        let a = (f32::from(self.a) * opacity.clamp(0.0, 1.0)).round();
        Self { a: a as u8, ..self }
    }

    /// `#rgb`, `#rrggbb` or `#rrggbbaa` (with or without the `#`).
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        let digits = text.strip_prefix('#').unwrap_or(text);
        if !digits.is_ascii() {
            return None;
        }
        let value = |s: &str| u8::from_str_radix(s, 16).ok();
        let (r, g, b, a) = match digits.len() {
            3 => {
                let d = |i: usize| value(&digits[i..=i]).map(|v| v * 17);
                (d(0)?, d(1)?, d(2)?, 255)
            }
            6 => (
                value(&digits[0..2])?,
                value(&digits[2..4])?,
                value(&digits[4..6])?,
                255,
            ),
            8 => (
                value(&digits[0..2])?,
                value(&digits[2..4])?,
                value(&digits[4..6])?,
                value(&digits[6..8])?,
            ),
            _ => return None,
        };
        Some(Self { r, g, b, a })
    }
}

/// `a * b / 255`, rounded.
#[inline]
pub const fn mul255(a: u32, b: u32) -> u32 {
    let t = a * b + 128;
    (t + (t >> 8)) >> 8
}
