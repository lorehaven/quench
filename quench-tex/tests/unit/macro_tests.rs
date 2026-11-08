use quench_tex::{Error, Macros, parse, parse_with, to_unicode};

fn text(tex: &str) -> String {
    to_unicode(&parse(tex).unwrap())
}

#[quench_test::test]
fn newcommand_without_arguments() {
    assert_eq!(text(r"\newcommand{\R}{\mathbb{R}} x \in \R"), "x ∈ ℝ");
    assert_eq!(text(r"\newcommand\N{\mathbb{N}} \N"), "ℕ");
}

#[quench_test::test]
fn newcommand_with_arguments() {
    assert_eq!(
        text(r"\newcommand{\norm}[1]{\left\|#1\right\|} \norm{x}"),
        "‖x‖"
    );
    assert_eq!(
        text(r"\newcommand{\pair}[2]{(#1, #2)} \pair{a}{b} \pair xy"),
        "(a, b)(x, y)"
    );
    assert_eq!(text(r"\newcommand{\sq}[1]{{#1}^2} \sq{\sq{x}}"), "x²²");
}

#[quench_test::test]
fn newcommand_with_an_optional_argument() {
    let defined = r"\newcommand{\lp}[2][p]{\|#2\|_#1}";
    assert_eq!(text(&format!(r"{defined} \lp{{f}}")), "‖f‖ₚ");
    assert_eq!(text(&format!(r"{defined} \lp[2]{{f}}")), "‖f‖₂");
}

#[quench_test::test]
fn renewcommand_and_providecommand() {
    assert_eq!(text(r"\newcommand{\a}{1}\renewcommand{\a}{2}\a"), "2");
    assert_eq!(text(r"\newcommand{\a}{1}\providecommand{\a}{2}\a"), "1");
    assert_eq!(
        text(r"\renewcommand{\frac}[2]{#1:#2} \frac{a}{b}"),
        "a : b",
        "a user macro shadows a built-in"
    );
}

#[quench_test::test]
fn def_with_and_without_parameters() {
    assert_eq!(text(r"\def\e{\varepsilon} \e > 0"), "ε > 0");
    assert_eq!(text(r"\def\abs#1{\lvert #1\rvert} \abs{x}"), "|x|");
    assert_eq!(text(r"\def\f#1#2{#2#1} \f ab"), "ba");
    assert!(
        matches!(parse(r"\def\a#1.{#1}"), Err(Error::Unsupported { command, .. }) if command == r"\def")
    );
}

#[quench_test::test]
fn declare_math_operator() {
    assert_eq!(text(r"\DeclareMathOperator{\tr}{tr} \tr A"), "tr A");
    assert_eq!(
        text(r"\DeclareMathOperator*{\esssup}{ess\,sup} \esssup_x f"),
        "ess supₓ f"
    );
}

#[quench_test::test]
fn definitions_carry_across_formulas_of_a_page() {
    let mut macros = Macros::new();
    parse_with(r"\newcommand{\R}{\mathbb{R}}", &mut macros).unwrap();
    assert!(macros.contains("R"));
    let formula = parse_with(r"f\colon \R \to \R", &mut macros).unwrap();
    assert_eq!(to_unicode(&formula), "f: ℝ → ℝ");
    // A formula that fails leaves the definitions as they were.
    assert!(parse_with(r"\newcommand{\Z}{\mathbb{Z}} \unknown", &mut macros).is_err());
    assert!(!macros.contains("Z"));
    assert_eq!(macros.len(), 1);
}

#[quench_test::test]
fn macros_can_be_defined_from_code() {
    let mut macros = Macros::new();
    macros.define("abs", 1, r"\left|#1\right|").unwrap();
    assert_eq!(
        to_unicode(&parse_with(r"\abs{-1}", &mut macros).unwrap()),
        "|−1|"
    );
    assert!(macros.define("bad", 1, "#2").is_err());
}

#[quench_test::test]
fn recursive_macros_are_an_error_not_a_hang() {
    for tex in [
        r"\def\a{\a} \a",
        r"\newcommand{\b}{x\b} \b",
        r"\def\c{\c\c} \c",
        r"\def\d{{\d}} \d",
    ] {
        assert!(matches!(parse(tex), Err(Error::Syntax { .. })), "{tex}");
    }
}

#[quench_test::test]
fn bad_definitions_are_errors() {
    assert!(parse(r"\newcommand{\a}[1]{#2}").is_err());
    assert!(parse(r"\newcommand{\a}[x]{1}").is_err());
    assert!(parse(r"\newcommand{\a}").is_err());
    assert!(
        parse(r"\newcommand{\a}[1]{#1} \a").is_err(),
        "a missing argument"
    );
    assert!(parse(r"\newcommand{\a}{{} \a").is_err());
}
