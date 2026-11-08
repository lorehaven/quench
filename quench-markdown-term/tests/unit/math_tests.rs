use crate::assert_fits;
use quench_html::markdown::{Options as Syntax, parse};
use quench_markdown_term::{
    FormulaSpot, MathMode, MathText, Options, Role, SourceMath, render_with_math, to_plain,
};

/// Knows `x^2` and `\frac{a}{b}`, nothing else.
struct Known;

impl MathText for Known {
    fn inline(&self, tex: &str) -> Option<String> {
        (tex == "x^2").then(|| "x²".to_owned())
    }

    fn display(&self, tex: &str) -> Option<Vec<String>> {
        (tex.trim() == r"\frac{a}{b}").then(|| vec!["a".to_owned(), "─".to_owned(), "b".to_owned()])
    }
}

fn render(markdown: &str, options: &Options) -> quench_markdown_term::Rendered {
    let rendered = render_with_math(&parse(markdown, Syntax::ALL), options, &Known);
    assert_fits(&rendered, options.width, "math");
    rendered
}

const PAGE: &str = "Inline $x^2$ and $y_1$ here.\n\n$$\n\\frac{a}{b}\n$$\n\n$$\n\\unknown{z}\n$$\n";

#[quench_test::test]
fn the_hook_is_used_and_unknown_formulas_show_their_source() {
    let rendered = render(
        PAGE,
        &Options {
            width: 20,
            ..Options::default()
        },
    );
    assert_eq!(
        to_plain(&rendered),
        "Inline x² and $y_1$\nhere.\n\n         a\n         ─\n         b\n\n\
         ╭─ tex ────────────╮\n│ \\unknown{z}      │\n╰──────────────────╯\n"
    );
    let source = rendered.lines[0]
        .spans
        .iter()
        .find(|s| s.text == "$y_1$")
        .unwrap();
    assert!(source.style.dim);
    assert_eq!(source.style.role, Role::Math);
}

#[quench_test::test]
fn formulas_record_where_they_ended_up() {
    let rendered = render(
        PAGE,
        &Options {
            width: 20,
            ..Options::default()
        },
    );
    let spot = |line, column, width, height, tex: &str, display| FormulaSpot {
        line,
        column,
        width,
        height,
        tex: tex.to_owned(),
        display,
    };
    assert_eq!(
        rendered.formulas,
        [
            spot(0, 7, 2, 1, "x^2", false),
            spot(0, 14, 5, 1, "y_1", false),
            spot(3, 9, 1, 3, "\\frac{a}{b}", true),
            spot(7, 0, 20, 3, "\\unknown{z}", true),
        ]
    );
}

#[quench_test::test]
fn source_mode_ignores_the_hook() {
    let options = Options {
        width: 30,
        math: MathMode::Source,
        ..Options::default()
    };
    let text = to_plain(&render(PAGE, &options));
    assert!(
        text.starts_with("Inline $x^2$ and $y_1$ here.\n\n╭─ tex "),
        "{text}"
    );
    assert!(text.contains("│ \\frac{a}{b}"));
}

#[quench_test::test]
fn source_math_translates_nothing() {
    assert_eq!(SourceMath.inline("x"), None);
    assert_eq!(SourceMath.display("x"), None);
}

#[quench_test::test]
fn display_text_wider_than_the_line_falls_back_to_source() {
    struct Wide;
    impl MathText for Wide {
        fn inline(&self, _: &str) -> Option<String> {
            None
        }
        fn display(&self, _: &str) -> Option<Vec<String>> {
            Some(vec!["x".repeat(50)])
        }
    }
    let options = Options {
        width: 20,
        ..Options::default()
    };
    let rendered = render_with_math(&parse("$$\na\n$$\n", Syntax::ALL), &options, &Wide);
    assert_eq!(
        to_plain(&rendered),
        "╭─ tex ────────────╮\n│ a                │\n╰──────────────────╯\n"
    );
}
