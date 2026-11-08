//! [`TexMath`]: the [`MathText`] of quench-tex, formulas as Unicode inline and as two-dimensional text in
//! display (stacked fractions, limits above and below, tall delimiters).
//!
//! A formula quench-tex does not read (an unknown command, a syntax error) gives `None`, so the renderer shows
//! its TeX source instead of something half-translated. Macros a formula defines (`\newcommand`, `\def`) are
//! kept for the formulas after it, as on a page typeset by `MathJax`: use one `TexMath` per page.

use crate::MathText;
use quench_tex::Macros;
use std::cell::RefCell;

/// Formulas as text through quench-tex, with the macros of the page so far.
#[derive(Debug, Default)]
pub struct TexMath {
    macros: RefCell<Macros>,
}

impl TexMath {
    /// A `TexMath` with no macros defined yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn read(&self, tex: &str) -> Option<quench_tex::Formula> {
        quench_tex::parse_with(tex, &mut self.macros.borrow_mut()).ok()
    }
}

impl MathText for TexMath {
    fn inline(&self, tex: &str) -> Option<String> {
        self.read(tex)
            .map(|formula| quench_tex::to_unicode(&formula))
    }

    fn display(&self, tex: &str) -> Option<Vec<String>> {
        self.read(tex)
            .map(|formula| quench_tex::to_text_block(&formula).lines)
    }
}
