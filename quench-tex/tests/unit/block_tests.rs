use quench_tex::{parse, to_text_block};

/// Checks the block of `tex` against its rows, its baseline row and its width.
fn check(tex: &str, expected: &[&str], baseline: usize, width: usize) {
    let block = to_text_block(&parse(tex).unwrap());
    assert_eq!(
        block.lines.join("\n"),
        expected.join("\n"),
        "{tex}\n{}",
        block.lines.join("\n")
    );
    assert_eq!((block.baseline, block.width), (baseline, width), "{tex}");
}

#[quench_test::test]
fn the_quadratic_formula() {
    check(
        r"x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}",
        &[
            "          ________",
            "    −b ± √b² − 4ac",
            "x = ──────────────",
            "          2a",
        ],
        2,
        18,
    );
}

#[quench_test::test]
fn a_fraction_stacks_over_a_rule() {
    check(r"\frac{a}{b}", &["a", "─", "b"], 1, 1);
}

#[quench_test::test]
fn nested_fractions_stack() {
    check(
        r"\frac{1}{1+\frac{1}{x}}",
        &["  1", "─────", "    1", "1 + ─", "    x"],
        1,
        5,
    );
}

#[quench_test::test]
fn long_scripts_are_raised_and_lowered() {
    check(r"x^{n+1} + y_{ij}", &[" n+1", "x    + yᵢⱼ"], 1, 10);
}

#[quench_test::test]
fn short_scripts_stay_inline() {
    check(r"x^2 + a_{i}", &["x² + aᵢ"], 0, 7);
}

#[quench_test::test]
fn a_script_without_unicode_forms_is_raised() {
    check(r"e^{i\pi} = -1", &[" iπ", "e   = −1"], 1, 8);
}

#[quench_test::test]
fn a_sum_has_its_limits_above_and_below() {
    check(
        r"\sum_{i=1}^{n} i = \frac{n(n+1)}{2}",
        &[" n      n(n + 1)", " ∑  i = ────────", "i=1        2"],
        1,
        16,
    );
}

#[quench_test::test]
fn an_integral_has_its_limits_beside() {
    check(r"\int_0^1 f(x)\,dx", &[" 1", "∫  f(x) dx", " 0"], 1, 10);
}

#[quench_test::test]
fn an_integral_with_raised_scripts() {
    check(
        r"\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}",
        &[" ∞   −x²      _", "∫   e   dx = √π", " −∞"],
        1,
        15,
    );
}

#[quench_test::test]
fn lim_takes_limits_below_in_display() {
    check(
        r"\lim_{x\to 0} \frac{\sin x}{x} = 1",
        &["    sin x", "lim ───── = 1", "x→0   x"],
        1,
        13,
    );
}

#[quench_test::test]
fn a_pmatrix() {
    check(
        r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
        &["⎛a  b⎞", "⎝c  d⎠"],
        0,
        6,
    );
}

#[quench_test::test]
fn a_bmatrix() {
    check(
        r"\begin{bmatrix} 1 & 0 & 0 \\ 0 & 1 & 0 \\ 0 & 0 & 1 \end{bmatrix}",
        &["⎡1  0  0⎤", "⎢0  1  0⎥", "⎣0  0  1⎦"],
        1,
        9,
    );
}

#[quench_test::test]
fn a_vmatrix() {
    check(
        r"\det\begin{vmatrix} a & b \\ c & d \end{vmatrix} = ad - bc",
        &["det │a  b│ = ad − bc", "    │c  d│"],
        0,
        20,
    );
}

#[quench_test::test]
fn cases_with_a_two_row_brace() {
    check(
        r"|x| = \begin{cases} x & x \ge 0 \\ -x & \text{otherwise} \end{cases}",
        &["|x| = ⎰ x   x ≥ 0", "      ⎱ −x  otherwise"],
        0,
        21,
    );
}

#[quench_test::test]
fn a_radical_has_a_rule_over_the_radicand() {
    check(r"\sqrt{x+1}", &[" _____", "√x + 1"], 1, 6);
}

#[quench_test::test]
fn a_cube_root_of_a_fraction() {
    check(r"\sqrt[3]{\frac{a}{b}}", &[" _", "│a", "│─", "∛b"], 2, 2);
}

#[quench_test::test]
fn aligned_rows_align_at_the_relation() {
    check(
        r"\begin{aligned} f(x) &= (x+1)^2 \\ &= x^2 + 2x + 1 \end{aligned}",
        &["f(x) = (x + 1)²", "     = x² + 2x + 1"],
        0,
        18,
    );
}

#[quench_test::test]
fn a_superscript_on_a_tall_delimiter_sits_at_its_top() {
    check(
        r"\left( \frac{a}{b} \right)^2",
        &["⎛a⎞2", "⎜─⎟", "⎝b⎠"],
        1,
        4,
    );
}

#[quench_test::test]
fn middle_grows_with_its_left_and_right() {
    check(
        r"\left\{ \frac{a}{b} \middle| c \right\}",
        &["⎧a │  ⎫", "⎨─ │ c⎬", "⎩b │  ⎭"],
        1,
        7,
    );
}

#[quench_test::test]
fn an_overbrace_with_a_label() {
    check(
        r"\overbrace{a+b+c}^{3}",
        &["    3", "╭───┴───╮", "a + b + c"],
        2,
        9,
    );
}

#[quench_test::test]
fn a_binomial_stacks_in_parentheses() {
    check(r"\binom{n}{k}", &["⎛n⎞", "⎜ ⎟", "⎝k⎠"], 1, 3);
}

#[quench_test::test]
fn an_array_with_rules() {
    check(
        r"\begin{array}{c|c} 1 & 2 \\ \hline 3 & 4 \end{array}",
        &["1 │ 2", "──┼──", "3 │ 4"],
        1,
        5,
    );
}

#[quench_test::test]
fn wide_accents_draw_a_row() {
    check(r"\vec{AB} + \overline{xy}", &["─→   __", "AB + xy"], 1, 7);
}

#[quench_test::test]
fn a_product_with_a_fraction() {
    check(
        r"\prod_{p} \frac{1}{1-p^{-s}}",
        &["     1", "∏ ───────", "p 1 − p⁻ˢ"],
        1,
        9,
    );
}

#[quench_test::test]
fn gathered_rows_are_centred() {
    check(
        r"\begin{gathered} a = b \\ c + d = e \end{gathered}",
        &["  a = b", "c + d = e"],
        0,
        9,
    );
}

#[quench_test::test]
fn a_bmatrix_with_braces() {
    check(
        r"\begin{Bmatrix} x \\ y \\ z \end{Bmatrix}",
        &["⎧x⎫", "⎨y⎬", "⎩z⎭"],
        1,
        3,
    );
}

#[quench_test::test]
fn a_double_bar_matrix() {
    check(
        r"\begin{Vmatrix} a & b \\ c & d \end{Vmatrix}",
        &["‖a  b‖", "‖c  d‖"],
        0,
        6,
    );
}

#[quench_test::test]
fn cases_grow_with_a_fraction() {
    check(
        r"f(x) = \begin{cases} 0 & x < 0 \\ \frac{x}{2} & 0 \le x \le 1 \\ 1 & x > 1 \end{cases}",
        &[
            "       ⎧ 0  x < 0",
            "       ⎪",
            "       ⎪ x",
            "f(x) = ⎨ ─  0 ≤ x ≤ 1",
            "       ⎪ 2",
            "       ⎪",
            "       ⎩ 1  x > 1",
        ],
        3,
        21,
    );
}

#[quench_test::test]
fn an_infinite_sum() {
    check(
        r"\sum_{n=0}^{\infty} \frac{x^n}{n!}",
        &[" ∞  xⁿ", " ∑  ──", "n=0 n!"],
        1,
        6,
    );
}

#[quench_test::test]
fn an_underbrace_with_a_label() {
    check(
        r"\underbrace{1 + \cdots + 1}_{n}",
        &["1 + ⋯ + 1", "╰───┬───╯", "    n"],
        0,
        9,
    );
}
