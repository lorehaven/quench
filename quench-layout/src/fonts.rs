//! The faces text is measured and drawn with, and the vertical metrics a line is built from.

use std::cell::RefCell;
use std::collections::HashMap;

use quench_css::ComputedStyle;
use quench_text::FontSet;

/// Regular and bold Noto Sans, each with the machine's fonts behind it for what Noto lacks.
pub struct Fonts {
    pub regular: FontSet,
    pub bold: FontSet,
    spaces: RefCell<HashMap<(u32, bool), f32>>,
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
            regular: FontSet::noto_sans(),
            bold: FontSet::noto_sans_bold(),
            spaces: RefCell::default(),
        }
    }

    /// Noto Sans with the installed fonts as the fallback.
    #[must_use]
    pub fn system() -> Self {
        Self {
            regular: FontSet::noto_sans().with_system_fonts(),
            bold: FontSet::noto_sans_bold().with_system_fonts(),
            spaces: RefCell::default(),
        }
    }

    /// The face for a style: bold from weight 600.
    #[must_use]
    pub const fn set(&self, style: &ComputedStyle) -> &FontSet {
        if style.font_weight >= 600 {
            &self.bold
        } else {
            &self.regular
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
        let key = (style.font_size.to_bits(), style.font_weight >= 600);
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
