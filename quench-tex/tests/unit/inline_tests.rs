use quench_tex::{parse, to_unicode};

/// TeX and its inline Unicode text.
const CASES: &[(&str, &str)] = &[
    // Characters and classes.
    ("x", "x"),
    ("abc", "abc"),
    ("x+y", "x + y"),
    ("a-b", "a − b"),
    ("-x", "−x"),
    ("a=b", "a = b"),
    ("a<b>c", "a < b > c"),
    ("f(x)", "f(x)"),
    ("[0,1]", "[0, 1]"),
    ("x;y", "x; y"),
    ("n!", "n!"),
    ("a/b", "a/b"),
    ("|x|", "|x|"),
    ("3.14", "3.14"),
    ("a * b", "a * b"),
    ("(+1)", "(+1)"),
    // Primes and scripts.
    ("f'", "f′"),
    ("f''(x)", "f″(x)"),
    ("f'^2", "f′²"),
    ("x^2", "x²"),
    ("x_1", "x₁"),
    ("a_{ij}", "aᵢⱼ"),
    ("x^{n+1}", "xⁿ⁺¹"),
    ("x_i^2", "xᵢ²"),
    ("x^2_i", "xᵢ²"),
    ("e^{-x}", "e⁻ˣ"),
    ("x^*", "x^*"),
    ("x^{\\alpha}", "xᵅ"),
    ("e^{i\\pi}", "e^(iπ)"),
    ("a_{i,j}", "a_(i,j)"),
    ("{a+b}^2", "(a + b)²"),
    ("{}^{14}C", "¹⁴C"),
    ("A^T", "Aᵀ"),
    ("x_{\\text{max}}", "xₘₐₓ"),
    // Greek and symbols.
    ("\\alpha+\\beta", "α + β"),
    ("\\Gamma\\Delta\\Omega", "ΓΔΩ"),
    ("\\varepsilon\\epsilon\\varphi\\phi", "εϵφϕ"),
    ("a\\le b", "a ≤ b"),
    ("a\\ne b", "a ≠ b"),
    ("x\\approx y", "x ≈ y"),
    ("x\\in A", "x ∈ A"),
    ("A\\subseteq B", "A ⊆ B"),
    ("p\\implies q", "p ⟹ q"),
    ("f\\colon A\\to B", "f: A → B"),
    ("a\\times b", "a × b"),
    ("a\\cdot b", "a · b"),
    ("a\\pm b", "a ± b"),
    ("\\infty", "∞"),
    ("\\partial f", "∂f"),
    ("\\nabla\\cdot F", "∇ · F"),
    ("\\forall x\\exists y", "∀x∃y"),
    ("\\neg p", "¬p"),
    ("\\emptyset", "∅"),
    ("1,2,\\ldots,n", "1, 2, …, n"),
    ("a+\\cdots+z", "a + ⋯ + z"),
    ("\\hbar\\ell", "ℏℓ"),
    ("\\{x\\}", "{x}"),
    ("a\\not= b", "a ≠ b"),
    ("\\langle x\\rangle", "⟨x⟩"),
    // Large and named operators.
    ("\\sum_{i=1}^n i", "∑ᵢ₌₁ⁿ i"),
    ("\\prod_{k} a_k", "∏ₖ aₖ"),
    ("\\int_0^1 f", "∫₀¹ f"),
    ("\\iint", "∬"),
    ("\\oint_C", "∮_C"),
    ("\\bigcup_{i} A_i", "⋃ᵢ Aᵢ"),
    ("\\sin x", "sin x"),
    ("\\sin(x)", "sin(x)"),
    ("2\\cos\\theta", "2 cos θ"),
    ("\\log_2 n", "log₂ n"),
    ("\\lim_{x\\to0} f(x)", "lim_(x→0) f(x)"),
    ("\\limsup_{n} a_n", "lim supₙ aₙ"),
    ("\\max(a,b)", "max(a, b)"),
    ("\\operatorname{rank} A", "rank A"),
    ("\\operatorname*{argmax}_x f", "argmaxₓ f"),
    ("\\sum\\limits_{i} x", "∑ᵢ x"),
    // Fractions and binomials.
    ("\\frac{a}{b}", "a/b"),
    ("\\frac{a+b}{2}", "(a + b)/2"),
    ("\\frac{1}{x+1}", "1/(x + 1)"),
    ("\\frac{2x}{3}", "2x/3"),
    ("\\dfrac12", "1/2"),
    ("\\tfrac{\\pi}{2}", "π/2"),
    ("\\frac{1}{2}x", "(1/2)x"),
    ("\\frac{\\frac{a}{b}}{c}", "(a/b)/c"),
    ("\\binom{n}{k}", "(n k)"),
    ("\\dbinom{n}{2}", "(n 2)"),
    // Radicals.
    ("\\sqrt{x}", "√x"),
    ("\\sqrt{x+1}", "√(x + 1)"),
    ("\\sqrt{2x}", "√(2x)"),
    ("\\sqrt{12}", "√12"),
    ("\\sqrt[3]{x}", "∛x"),
    ("\\sqrt[4]{x}", "∜x"),
    ("\\sqrt[n]{x}", "ⁿ√x"),
    ("\\sqrt{(a+b)}", "√(a + b)"),
    // Delimiters.
    ("\\left(x\\right)", "(x)"),
    ("\\left[\\frac{a}{b}\\right]", "[a/b]"),
    ("\\left.\\frac{df}{dx}\\right|_{0}", "df/dx|₀"),
    ("\\left\\{x\\middle|x>0\\right\\}", "{x | x > 0}"),
    ("\\big(x\\big)", "(x)"),
    ("\\Bigl[x\\Bigr]", "[x]"),
    // Accents.
    ("\\hat x", "x\u{302}"),
    ("\\bar{x}", "x\u{304}"),
    ("\\vec{v}", "v\u{20d7}"),
    ("\\dot x", "x\u{307}"),
    ("\\ddot{x}", "x\u{308}"),
    ("\\tilde n", "n\u{303}"),
    ("\\overline{AB}", "A\u{305}B\u{305}"),
    ("\\underline{x}", "x\u{332}"),
    ("\\widehat{xy}", "x\u{302}y\u{302}"),
    ("\\overbrace{a+b}^{n}", "(a + b)ⁿ"),
    ("\\underbrace{x}_{k}", "xₖ"),
    ("\\stackrel{\\text{def}}{=}", "=ᵈᵉᶠ"),
    ("\\overset{!}{=}", "=^!"),
    // Fonts.
    ("\\mathbb{R}", "ℝ"),
    ("\\mathbb{NZQC}", "ℕℤℚℂ"),
    ("\\mathbb{A}", "𝔸"),
    ("\\mathcal{O}", "𝒪"),
    ("\\mathcal{BELM}", "ℬℰℒℳ"),
    ("\\mathscr{F}", "ℱ"),
    ("\\mathfrak{g}", "𝔤"),
    ("\\mathfrak{CHRZ}", "ℭℌℜℨ"),
    ("\\mathbf{v}", "𝐯"),
    ("\\boldsymbol{\\mu}", "𝛍"),
    ("\\mathsf{A}", "𝖠"),
    ("\\mathtt{x}", "𝚡"),
    ("\\mathrm{d}x", "dx"),
    ("\\mathit{ab}", "ab"),
    ("\\mathbb{1}", "𝟙"),
    // Text.
    ("\\text{if } x>0", "if x > 0"),
    ("\\text{for all $x$}", "for all x"),
    ("\\textbf{bold}", "bold"),
    ("x\\mbox{ and }y", "x and y"),
    ("\\text{a\\%}", "a%"),
    // Spacing.
    ("a\\,b", "a b"),
    ("a\\!b", "ab"),
    ("a\\quad b", "a  b"),
    ("a\\qquad b", "a    b"),
    ("a\\ b", "a b"),
    ("a~b", "a b"),
    ("\\int f(x)\\,dx", "∫ f(x) dx"),
    ("a\\hspace{1em}b", "a  b"),
    // Styles and colour.
    ("\\displaystyle\\frac{a}{b}", "a/b"),
    ("\\color{red} x+y", "x + y"),
    ("\\textcolor{blue}{x}=1", "x = 1"),
    ("\\mathrel{R}", "R"),
    ("a\\mathbin{\\#}b", "a # b"),
    // Arrays.
    ("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}", "(a b; c d)"),
    ("\\begin{bmatrix}1&0\\\\0&1\\end{bmatrix}", "[1 0; 0 1]"),
    ("\\begin{vmatrix}a&b\\\\c&d\\end{vmatrix}", "|a b; c d|"),
    (
        "\\left(\\begin{matrix}a\\\\b\\end{matrix}\\right)",
        "(a; b)",
    ),
    (
        "|x|=\\begin{cases}x&x\\ge0\\\\-x&\\text{otherwise}\\end{cases}",
        "|x| = {x, x ≥ 0; −x, otherwise}",
    ),
    ("\\begin{aligned}a&=b\\\\c&=d\\end{aligned}", "a = b; c = d"),
    ("\\begin{array}{cc}1&2\\end{array}", "1 2"),
    ("a\\\\b", "a; b"),
];

#[quench_test::test]
fn inline_text_matches_the_table() {
    let mut failures = Vec::new();
    for &(tex, expected) in CASES {
        match parse(tex) {
            Ok(formula) => {
                let got = to_unicode(&formula);
                if got != expected {
                    failures.push(format!("{tex:?}: expected {expected:?}, got {got:?}"));
                }
            }
            Err(error) => failures.push(format!("{tex:?}: {error}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}

#[quench_test::test]
fn the_table_covers_at_least_eighty_cases() {
    assert!(CASES.len() >= 80);
}
