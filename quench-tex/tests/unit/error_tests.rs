use quench_tex::{Error, parse};

fn syntax(tex: &str) -> (usize, String) {
    match parse(tex) {
        Err(Error::Syntax { at, message }) => (at, message),
        other => panic!("{tex}: expected a syntax error, got {other:?}"),
    }
}

#[quench_test::test]
fn an_unknown_command_is_named() {
    assert_eq!(
        parse(r"x + \foo{y}"),
        Err(Error::Unsupported {
            command: r"\foo".into(),
            at: 4
        })
    );
    assert_eq!(
        parse(r"\frac{\bar}{\qux}").unwrap_err(),
        Error::Syntax {
            at: 6,
            message: "missing argument for `\\bar`".into()
        }
    );
    assert!(
        matches!(parse(r"\frac{a}{\qux}"), Err(Error::Unsupported { command, at: 9 }) if command == r"\qux")
    );
    assert!(
        matches!(parse(r"\begin{tabular}{c}a\end{tabular}"), Err(Error::Unsupported { command, .. }) if command == r"\begin{tabular}")
    );
    assert!(
        matches!(parse(r"\not\alpha"), Err(Error::Unsupported { command, .. }) if command == r"\not")
    );
    assert!(
        matches!(parse(r"\text{\alpha}"), Err(Error::Unsupported { command, .. }) if command == r"\alpha")
    );
    let message = parse(r"a \foo").unwrap_err().to_string();
    assert!(message.contains(r"\foo"), "{message}");
}

#[quench_test::test]
fn unbalanced_braces() {
    assert_eq!(
        syntax("{x"),
        (0, "unbalanced braces: `{` without `}`".into())
    );
    assert_eq!(
        syntax("x}"),
        (1, "unbalanced braces: `}` without `{`".into())
    );
    assert_eq!(
        syntax(r"\frac{a}{b"),
        (8, "unbalanced braces: `{` without `}`".into())
    );
    assert!(syntax(r"\text{abc").1.contains("unbalanced"));
}

#[quench_test::test]
fn misplaced_structure() {
    assert!(syntax(r"x \right)").1.contains(r"\right` without `\left"));
    assert!(syntax(r"\left( x").1.contains(r"\left` without `\right"));
    assert!(syntax(r"a \middle| b").1.contains(r"\middle"));
    assert!(syntax("a & b").1.contains("`&` outside an array"));
    assert!(syntax(r"\end{matrix}").1.contains(r"\end"));
    assert!(syntax(r"\begin{matrix} a").1.contains("without"));
    assert!(
        syntax(r"\begin{matrix} a \end{pmatrix}")
            .1
            .contains("ended by")
    );
    assert!(syntax(r"\left x \right)").1.contains("delimiter"));
    assert!(syntax(r"x \limits").1.contains("operator"));
    assert!(syntax(r"\hline").1.contains("outside an array"));
}

#[quench_test::test]
fn missing_arguments_and_double_scripts() {
    assert!(
        syntax(r"\frac{a}")
            .1
            .contains(r"missing argument for `\frac`")
    );
    assert!(syntax(r"\sqrt").1.contains("missing argument"));
    assert!(syntax("x^").1.contains("missing argument"));
    assert!(syntax("{x^}").1.contains("missing argument"));
    assert_eq!(syntax("x^2^3"), (3, "double superscript".into()));
    assert_eq!(syntax("x_1_2"), (3, "double subscript".into()));
    assert!(syntax("x^2'").1.contains("double superscript"));
    assert!(parse("x'^2").is_ok() && parse("x^2_1").is_ok());
}

#[quench_test::test]
fn other_syntax_errors() {
    assert!(syntax("\\").1.contains("lone"));
    assert!(syntax("#1").1.contains("parameter"));
    assert!(syntax("a $ b").1.contains('$'));
    assert!(syntax(r"\text{$x}").1.contains('$'));
    assert!(syntax(r"\hspace{wide}").1.contains("length"));
    assert!(
        syntax(r"\begin{array}{p}a\end{array}")
            .1
            .contains("columns")
    );
    assert!(syntax(&"{".repeat(500)).1.contains("nested too deeply"));
    assert!(
        syntax(&r"\sqrt".repeat(500))
            .1
            .contains("nested too deeply")
    );
}

#[quench_test::test]
fn comments_and_spaces_are_ignored_in_math() {
    assert_eq!(parse("a % a comment\n + b"), parse("a+b"));
    assert_eq!(parse(r"\alpha   \beta"), parse(r"\alpha\beta"));
}
