//! The faces text is measured and drawn with, and the vertical metrics a line is built from.

use std::cell::RefCell;
use std::collections::HashMap;

use quench_css::ComputedStyle;
use quench_text::FontSet;

/// Light, regular, medium and bold Noto Sans, each with the machine's fonts behind it for what Noto lacks.
pub struct Fonts {
    pub light: FontSet,
    pub regular: FontSet,
    pub medium: FontSet,
    pub bold: FontSet,
    spaces: RefCell<HashMap<(u32, usize), f32>>,
}

/// The vertical metrics of a font at a size, rounded to whole pixels as browsers do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub ascent: f32,
    pub descent: f32,
    /// The height of a line with `line-height: normal`.
    pub normal: f32,
}

impl Fonts {
    /// Noto Sans with no fallback: the same everywhere, which is what tests want.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            light: FontSet::noto_sans_light(),
            regular: FontSet::noto_sans(),
            medium: FontSet::noto_sans_medium(),
            bold: FontSet::noto_sans_bold(),
            spaces: RefCell::default(),
        }
    }

    /// Noto Sans with the installed fonts as the fallback.
    #[must_use]
    pub fn system() -> Self {
        Self {
            light: FontSet::noto_sans_light().with_system_fonts(),
            regular: FontSet::noto_sans().with_system_fonts(),
            medium: FontSet::noto_sans_medium().with_system_fonts(),
            bold: FontSet::noto_sans_bold().with_system_fonts(),
            spaces: RefCell::default(),
        }
    }

    /// Which of the four faces a weight uses: light up to 350, regular to 449, medium to 599, bold from 600.
    const fn face_of(weight: u16) -> usize {
        match weight {
            0..=350 => 0,
            351..=449 => 1,
            450..=599 => 2,
            _ => 3,
        }
    }

    /// The face for a style (see the weights above).
    #[must_use]
    pub const fn set(&self, style: &ComputedStyle) -> &FontSet {
        match Self::face_of(style.font_weight) {
            0 => &self.light,
            1 => &self.regular,
            2 => &self.medium,
            _ => &self.bold,
        }
    }

    #[must_use]
    pub fn metrics(&self, style: &ComputedStyle) -> Metrics {
        let layout = self.set(style).layout("", style.font_size, None);
        let ascent = layout.ascent.round();
        let descent = (layout.line_height - layout.ascent).round();
        Metrics {
            ascent,
            descent,
            normal: ascent + descent,
        }
    }

    #[must_use]
    pub fn measure(&self, style: &ComputedStyle, text: &str) -> f32 {
        self.set(style).measure(text, style.font_size)
    }

    /// The width of a space (measured between two letters, as a line's trailing space counts for nothing).
    #[must_use]
    pub fn space(&self, style: &ComputedStyle) -> f32 {
        let key = (style.font_size.to_bits(), Self::face_of(style.font_weight));
        if let Some(w) = self.spaces.borrow().get(&key) {
            return *w;
        }
        let w = self.measure(style, "x x") - self.measure(style, "xx");
        self.spaces.borrow_mut().insert(key, w);
        w
    }

    /// The line height of a style in pixels.
    #[must_use]
    pub fn line_height(&self, style: &ComputedStyle) -> f32 {
        use quench_css::LineHeight;
        match style.line_height {
            LineHeight::Normal => self.metrics(style).normal,
            LineHeight::Number(n) => style.font_size * n,
            LineHeight::Px(px) => px,
        }
    }
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").finish_non_exhaustive()
    }
}
