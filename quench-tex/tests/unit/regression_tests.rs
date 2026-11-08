use quench_tex::{
    Class, Error, Limits, Macros, Node, parse, parse_with, to_text_block, to_unicode,
};

/// The colour of the first node, which must be a colour change.
fn colour(tex: &str) -> String {
    match parse(tex).unwrap().body.into_iter().next() {
        Some(Node::Color { color, .. }) => color,
        other => panic!("{tex}: {other:?}"),
    }
}

#[quench_test::test]
fn hex_colours_are_read_as_written_whatever_digit_they_start_with() {
    // `#` and a digit is a macro parameter to the lexer; in a colour it is the colour.
    assert_eq!(colour(r"\color{#1a2b3c} x"), "#1a2b3c");
    assert_eq!(colour(r"\color{#123} x"), "#123");
    assert_eq!(colour(r"\color{#ff0000} x"), "#ff0000");
    assert_eq!(colour(r"\color{#0F0F0F80} x"), "#0F0F0F80");
    assert_eq!(colour(r"\textcolor{#1a2b3c}{x}"), "#1a2b3c");
    assert_eq!(colour(r"\textcolor{ #9 9 9 }{x}"), "#999");
    assert_eq!(colour(r"\color[HTML]{1A2B3C} x"), "HTML:1A2B3C");
    assert_eq!(
        to_unicode(&parse(r"a + \color{#123456} b").unwrap()),
        "a + b"
    );
    // In a definition, `#1` is still the macro's parameter.
    let mut macros = Macros::new();
    parse_with(r"\newcommand{\paint}[2]{\textcolor{#1}{#2}}", &mut macros).unwrap();
    match parse_with(r"\paint{#00aa11}{x}", &mut macros)
        .unwrap()
        .body
        .as_slice()
    {
        [Node::Color { color, .. }] => assert_eq!(color, "#00aa11"),
        other => panic!("{other:?}"),
    }
    // What is not plain characters is still refused.
    assert!(matches!(
        parse(r"\color{\alpha} x"),
        Err(Error::Syntax { message, .. }) if message.contains("plain characters")
    ));
}

/// The base of the first node, which must be a script node.
fn script_base(tex: &str) -> Node {
    match parse(tex).unwrap().body.into_iter().next() {
        Some(Node::Scripts { base, .. }) => *base,
        other => panic!("{tex}: {other:?}"),
    }
}

#[quench_test::test]
fn limits_apply_to_mathop_as_to_a_built_in_operator() {
    for (tex, limits) in [
        (r"\mathop{x}_a", Limits::Display),
        (r"\mathop{x}\limits_a", Limits::Always),
        (r"\mathop{x}\nolimits_a", Limits::Never),
        (r"\mathop{x}\nolimits\displaylimits_a", Limits::Display),
    ] {
        match script_base(tex) {
            Node::Class {
                class: Class::Op,
                limits: got,
                ..
            } => assert_eq!(got, Some(limits), "{tex}"),
            other => panic!("{tex}: {other:?}"),
        }
    }
    // The other classes have no limits to set.
    assert!(matches!(
        parse(r"\mathrel{x}").unwrap().body.as_slice(),
        [Node::Class { limits: None, .. }]
    ));
    assert!(parse(r"\mathrel{R}\limits_a").is_err());
    assert!(parse(r"\mathbin{+}\nolimits").is_err());
    // `\operatorname*` is an operator already, and takes them too.
    assert!(matches!(
        script_base(r"\operatorname*{argmax}\nolimits_x"),
        Node::Op {
            limits: Limits::Never,
            ..
        }
    ));
    assert!(matches!(
        script_base(r"\operatorname{rank}\limits_x"),
        Node::Op {
            limits: Limits::Always,
            ..
        }
    ));
}

#[quench_test::test]
fn mathop_limits_move_the_scripts_in_a_block() {
    // Display style: above and below by default and with `\limits`; beside with `\nolimits`.
    let below = to_text_block(&parse(r"\mathop{T}_{i=1}").unwrap());
    let limits = to_text_block(&parse(r"\mathop{T}\limits_{i=1}").unwrap());
    let beside = to_text_block(&parse(r"\mathop{T}\nolimits_{i=1}").unwrap());
    assert_eq!(below.lines, limits.lines);
    assert_eq!(below.lines.len(), 2, "{:?}", below.lines);
    assert_eq!(beside.lines.len(), 2, "{:?}", beside.lines);
    assert_ne!(below.lines, beside.lines);
    assert!(beside.lines[0].starts_with('T'), "{:?}", beside.lines);
}

#[quench_test::test]
fn macros_defined_from_code_take_at_most_nine_arguments() {
    let mut macros = Macros::new();
    assert!(macros.define("nine", 9, "#9").is_ok());
    let error = macros.define("ten", 10, "x").unwrap_err();
    assert_eq!(
        error,
        Error::Syntax {
            at: 0,
            message: r"`\ten` takes 0 to 9 arguments, not `10`".into()
        }
    );
    assert!(macros.define("many", u8::MAX, "x").is_err());
    assert!(!macros.contains("ten") && !macros.contains("many"));
    assert_eq!(macros.len(), 1);
}

#[quench_test::test]
fn every_accent_has_a_spacing_form_that_is_not_combining() {
    use quench_tex::{Accent, display_width};
    for accent in [
        Accent::Hat,
        Accent::Check,
        Accent::Tilde,
        Accent::Acute,
        Accent::Grave,
        Accent::Dot,
        Accent::Ddot,
        Accent::Dddot,
        Accent::Breve,
        Accent::Bar,
        Accent::Vec,
        Accent::Ring,
        Accent::WideHat,
        Accent::WideTilde,
        Accent::WideCheck,
        Accent::OverRightArrow,
        Accent::OverLeftArrow,
        Accent::OverLeftRightArrow,
    ] {
        let spacing = accent.spacing().to_string();
        assert_eq!(display_width(&spacing), 1, "{accent:?}: {spacing:?}");
        assert_ne!(accent.spacing(), accent.combining(), "{accent:?}");
    }
    assert_eq!(Accent::Dddot.spacing(), '…');
    // Over a wide base the block draws the spacing form on its own row, one cell wide.
    let block = to_text_block(&parse(r"\dddot{xyz}").unwrap());
    assert_eq!(block.lines, [" …", "xyz"]);
    assert_eq!(block.width, 3);
}
