//! Random formulas: whatever parses typesets (the font has every glyph these use), never panics, and keeps its
//! ink inside its box vertically.

use quench_tex::typeset::{RenderOptions, SvgOptions, to_rgba, to_svg, typeset};

const PIECES: &[&str] = &[
    "x",
    "2",
    "+",
    "-",
    "=",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "^",
    "_",
    "'",
    "&",
    r"\\",
    ",",
    "|",
    " ",
    r"\frac",
    r"\dfrac",
    r"\binom",
    r"\sqrt",
    r"\sqrt[3]",
    r"\left(",
    r"\right)",
    r"\left.",
    r"\right|",
    r"\middle|",
    r"\big(",
    r"\Bigr]",
    r"\sum",
    r"\int",
    r"\lim",
    r"\limits",
    r"\nolimits",
    r"\alpha",
    r"\le",
    r"\to",
    r"\cdot",
    r"\infty",
    r"\hat",
    r"\vec",
    r"\widehat",
    r"\overline",
    r"\underline",
    r"\overbrace",
    r"\underbrace",
    r"\stackrel",
    r"\overset",
    r"\mathbb",
    r"\mathcal",
    r"\mathbf",
    r"\mathrm",
    r"\text{a b}",
    r"\textbf",
    r"\operatorname",
    r"\displaystyle",
    r"\scriptstyle",
    r"\color{red}",
    r"\,",
    r"\quad",
    r"\!",
    r"\begin{pmatrix}",
    r"\end{pmatrix}",
    r"\begin{cases}",
    r"\end{cases}",
    r"\begin{aligned}",
    r"\end{aligned}",
    r"\begin{array}{c|l}",
    r"\end{array}",
    r"\hline",
    r"\ldots",
    r"\cdots",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[quench_test::test]
fn random_formulas_typeset_whole() {
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    let mut typeset_count = 0;
    let options = RenderOptions {
        pixels_per_em: 12.0,
        ..RenderOptions::default()
    };
    for n in 0..3000 {
        let len = 1 + rng.below(14);
        let tex: String = (0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect();
        let Ok(formula) = quench_tex::parse(&tex) else {
            continue;
        };
        let set = typeset(&formula, n % 2 == 0).unwrap_or_else(|e| panic!("{tex:?}: {e}"));
        if let Some((_, y0, _, y1)) = set.ink() {
            assert!(
                y1 <= set.ascent + 0.5 && -y0 <= set.descent + 0.5,
                "{tex:?}"
            );
        }
        assert!(set.width.is_finite() && set.ascent.is_finite() && set.descent.is_finite());
        if n % 10 == 0 {
            let _ = to_rgba(&set, &options);
            let _ = to_svg(&set, &SvgOptions::default());
        }
        typeset_count += 1;
    }
    assert!(typeset_count > 200, "only {typeset_count} parsed");
}
