//! The renderer with quench-tex's text output for its formulas.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used)]

use quench_html::markdown::{Options as Markdown, parse};
use quench_markdown_term::{MathText, Options, TexMath, render_with_math, to_plain};

fn page(text: &str, width: usize) -> String {
    let document = parse(text, Markdown::ALL);
    let options = Options {
        width,
        ..Options::default()
    };
    to_plain(&render_with_math(&document, &options, &TexMath::new()))
}

#[quench_test::test]
fn inline_formulas_become_unicode() {
    let out = page("Area $\\pi r^2$, and $\\alpha \\le \\beta$.\n", 60);
    assert!(out.contains("πr²"), "{out}");
    assert!(out.contains("α ≤ β"), "{out}");
    assert!(!out.contains('$'), "{out}");
}

#[quench_test::test]
fn display_formulas_are_laid_out_in_two_dimensions() {
    let out = page("$$\n\\frac{a+b}{2}\n$$\n", 40);
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "{out}");
    assert!(lines[1].contains('─'), "a fraction rule: {out}");
    assert!(
        lines[0].contains("a + b") && lines[2].contains('2'),
        "{out}"
    );
}

#[quench_test::test]
fn what_quench_tex_does_not_read_is_shown_as_source() {
    let math = TexMath::new();
    assert_eq!(math.inline("\\boxed{x}"), None);
    assert_eq!(math.display("\\frac{1}{"), None);
    let out = page("See $\\boxed{x}$ here.\n", 60);
    assert!(out.contains("\\boxed{x}"), "{out}");
}

#[quench_test::test]
fn macros_carry_over_to_later_formulas() {
    let math = TexMath::new();
    assert_eq!(
        math.inline("\\newcommand{\\R}{\\mathbb{R}} x \\in \\R")
            .unwrap(),
        "x ∈ ℝ"
    );
    assert_eq!(math.inline("\\R^2").unwrap(), "ℝ²");
}

quench_test::main!();
