use quench_tex::{Macros, parse, parse_with, to_text_block, to_unicode};

/// TeX tokens that random formulas are built from.
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
    "!",
    " ",
    "~",
    "$",
    "#1",
    "%",
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
    r"\overline",
    r"\overbrace",
    r"\underbrace",
    r"\stackrel",
    r"\mathbb",
    r"\mathcal",
    r"\mathbf",
    r"\text{a b}",
    r"\text",
    r"\textbf",
    r"\operatorname",
    r"\displaystyle",
    r"\color{red}",
    r"\textcolor",
    r"\,",
    r"\quad",
    r"\hspace{1em}",
    r"\begin{pmatrix}",
    r"\end{pmatrix}",
    r"\begin{cases}",
    r"\end{cases}",
    r"\begin{aligned}",
    r"\end{aligned}",
    r"\begin{array}{c|l}",
    r"\end{array}",
    r"\hline",
    r"\newcommand{\m}[1]{#1^2}",
    r"\m",
    r"\def\n#1{(#1)}",
    r"\n",
    r"\not",
    r"\mathrel",
    r"\ldots",
    "α",
    "∑",
    r"\foo",
];

/// A small deterministic generator (xorshift), so a failure can be reproduced.
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

fn exercise(tex: &str, macros: &mut Macros) -> bool {
    match parse_with(tex, macros) {
        Ok(formula) => {
            let _ = to_unicode(&formula);
            let block = to_text_block(&formula);
            assert!(block.baseline < block.lines.len().max(1), "{tex:?}");
            assert!(
                block
                    .lines
                    .iter()
                    .all(|l| quench_tex::display_width(l) <= block.width),
                "{tex:?}"
            );
            true
        }
        Err(_) => false,
    }
}

#[quench_test::test]
fn random_formulas_never_panic() {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut macros = Macros::new();
    let mut accepted = 0;
    for _ in 0..4000 {
        let len = 1 + rng.below(14);
        let tex: String = (0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect();
        if exercise(&tex, &mut macros) {
            accepted += 1;
        }
    }
    assert!(
        accepted > 200,
        "only {accepted} of the random formulas parsed; the generator is too hostile"
    );
}

#[quench_test::test]
fn truncated_formulas_never_panic() {
    let formulas = [
        r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
        r"\left( \sum_{i=1}^{n} \frac{a_i}{b_i} \middle| x \right]",
        r"\begin{pmatrix} a & b \\ c & d \end{pmatrix} \begin{cases} 1 & x > 0 \\ 0 & \text{else $x$} \end{cases}",
        r"\newcommand{\abs}[1]{\left|#1\right|} \abs{\overbrace{a+b}^{n}} \sqrt[3]{\mathbb{R}^{n'}}",
        r"\begin{array}{c|c} \hline 1 & 2 \\ 3 & 4 \end{array} \hspace{2em} \textcolor{red}{x} \operatorname*{max}_x",
    ];
    let mut macros = Macros::new();
    for formula in formulas {
        for (end, _) in formula.char_indices() {
            exercise(&formula[..end], &mut macros);
            exercise(&formula[end..], &mut macros);
        }
        assert!(exercise(formula, &mut Macros::new()), "{formula}");
        assert!(parse(formula).is_ok());
    }
}

#[quench_test::test]
fn deep_nesting_within_the_limit_renders() {
    let deep = [
        format!("{}x{}", "{".repeat(70), "}".repeat(70)),
        format!("{}x{}", r"\sqrt{".repeat(40), "}".repeat(40)),
        format!("{}x{}", r"\frac{1}{".repeat(30), "}".repeat(30)),
        format!("{}x{}", "x^{".repeat(40), "}".repeat(40)),
        format!("{}x{}", r"\left(".repeat(40), r"\right)".repeat(40)),
    ];
    for tex in deep {
        assert!(exercise(&tex, &mut Macros::new()), "{tex}");
    }
}
