use quench_tex::{
    Accent, Align, ArrayKind, Class, Column, Limits, MathChar, MathFont, Node, OpName, Space,
    Style, TextFont, TextPart, display_width, large_op, math_alphanumeric, named_op, parse, symbol,
};

fn body(tex: &str) -> Vec<Node> {
    parse(tex).unwrap().body
}

fn ch(ch: char, class: Class) -> Node {
    Node::Char(MathChar { ch, class })
}

#[quench_test::test]
fn symbols_have_their_tex_classes() {
    let sample = [
        ("alpha", 'α', Class::Ord),
        ("Omega", 'Ω', Class::Ord),
        ("varepsilon", 'ε', Class::Ord),
        ("le", '≤', Class::Rel),
        ("ne", '≠', Class::Rel),
        ("in", '∈', Class::Rel),
        ("subseteq", '⊆', Class::Rel),
        ("mid", '∣', Class::Rel),
        ("to", '→', Class::Rel),
        ("Leftrightarrow", '⇔', Class::Rel),
        ("implies", '⟹', Class::Rel),
        ("pm", '±', Class::Bin),
        ("times", '×', Class::Bin),
        ("cdot", '⋅', Class::Bin),
        ("setminus", '∖', Class::Bin),
        ("oplus", '⊕', Class::Bin),
        ("infty", '∞', Class::Ord),
        ("partial", '∂', Class::Ord),
        ("emptyset", '∅', Class::Ord),
        ("vert", '|', Class::Ord),
        ("langle", '⟨', Class::Open),
        ("rangle", '⟩', Class::Close),
        ("lfloor", '⌊', Class::Open),
        ("ldots", '…', Class::Inner),
        ("cdots", '⋯', Class::Inner),
        ("vdots", '⋮', Class::Ord),
        ("colon", ':', Class::Punct),
    ];
    for (name, expected, class) in sample {
        assert_eq!(
            symbol(name),
            Some(MathChar {
                ch: expected,
                class
            }),
            "\\{name}"
        );
        assert_eq!(
            body(&format!("\\{name}")),
            [ch(expected, class)],
            "\\{name}"
        );
    }
    assert_eq!(symbol("sum"), None);
    assert_eq!(symbol("nosuchsymbol"), None);
}

#[quench_test::test]
fn typed_characters_have_their_tex_classes() {
    use Class::{Bin, Close, Open, Ord, Punct, Rel};
    let classes: Vec<Class> = body("x1+-=<(),;:!?|/.")
        .iter()
        .filter_map(Node::class)
        .collect();
    assert_eq!(
        classes,
        [
            Ord, Ord, Bin, Bin, Rel, Rel, Open, Close, Punct, Punct, Rel, Close, Close, Ord, Ord,
            Ord
        ]
    );
    assert_eq!(body("-"), [ch('−', Class::Bin)]);
    assert_eq!(
        body("≤"),
        [ch('≤', Class::Rel)],
        "a symbol typed as Unicode takes its command's class"
    );
}

#[quench_test::test]
fn operators_carry_their_limits_behaviour() {
    assert_eq!(large_op("sum"), Some(('∑', Limits::Display)));
    assert_eq!(large_op("int"), Some(('∫', Limits::Never)));
    assert_eq!(large_op("oint"), Some(('∮', Limits::Never)));
    assert_eq!(named_op("lim"), Some(("lim", Limits::Display)));
    assert_eq!(named_op("det"), Some(("det", Limits::Display)));
    assert_eq!(named_op("sin"), Some(("sin", Limits::Never)));
    assert_eq!(named_op("liminf"), Some(("lim inf", Limits::Display)));
    assert_eq!(
        body(r"\int\limits"),
        [Node::Op {
            name: OpName::Symbol('∫'),
            limits: Limits::Always
        }]
    );
    assert_eq!(
        body(r"\sum\nolimits"),
        [Node::Op {
            name: OpName::Symbol('∑'),
            limits: Limits::Never
        }]
    );
    assert_eq!(
        body(r"\operatorname*{arg\,max}"),
        [Node::Op {
            name: OpName::Named("arg max".into()),
            limits: Limits::Display
        }]
    );
}

#[quench_test::test]
fn scripts_attach_to_the_atom_before_them_in_either_order() {
    let expected = [Node::Scripts {
        base: Box::new(ch('x', Class::Ord)),
        sub: Some(vec![ch('i', Class::Ord)]),
        sup: Some(vec![ch('2', Class::Ord)]),
    }];
    assert_eq!(body("x_i^2"), expected);
    assert_eq!(body("x^2_i"), expected);
    assert_eq!(body("x^{2}_{i}"), expected);
    let [Node::Scripts { base, .. }] = &body("^2")[..] else {
        panic!()
    };
    assert_eq!(**base, Node::Group(vec![]));
}

#[quench_test::test]
fn primes_are_a_superscript_before_the_explicit_one() {
    let [Node::Scripts { sup: Some(sup), .. }] = &body("f''^2")[..] else {
        panic!()
    };
    assert_eq!(sup, &[ch('″', Class::Ord), ch('2', Class::Ord)]);
    let [Node::Scripts { sup: Some(sup), .. }] = &body("f'''")[..] else {
        panic!()
    };
    assert_eq!(sup, &[ch('‴', Class::Ord)]);
}

#[quench_test::test]
fn fractions_record_their_bar_delimiters_and_style() {
    let [
        Node::Frac {
            bar: true,
            open: None,
            close: None,
            style: Some(Style::Display),
            ..
        },
    ] = &body(r"\dfrac12")[..]
    else {
        panic!()
    };
    let [
        Node::Frac {
            bar: false,
            open: Some('('),
            close: Some(')'),
            style: Some(Style::Text),
            num,
            den,
        },
    ] = &body(r"\tbinom{n}{k}")[..]
    else {
        panic!()
    };
    assert_eq!((num.len(), den.len()), (1, 1));
}

#[quench_test::test]
fn radicals_keep_their_degree() {
    let [
        Node::Sqrt {
            degree: Some(degree),
            body: radicand,
        },
    ] = &body(r"\sqrt[n+1]{x}")[..]
    else {
        panic!()
    };
    assert_eq!(degree.len(), 3);
    assert_eq!(radicand, &[ch('x', Class::Ord)]);
    assert!(matches!(
        &body(r"\sqrt x")[..],
        [Node::Sqrt { degree: None, .. }]
    ));
}

#[quench_test::test]
fn left_right_and_middle_keep_every_delimiter() {
    let [
        Node::Fenced {
            open: Some('⟨'),
            body: inner,
            close: None,
        },
    ] = &body(r"\left\langle a \middle| b \right.")[..]
    else {
        panic!()
    };
    assert_eq!(inner[1], Node::Middle(Some('|')));
    assert_eq!(
        body(r"\Bigl( \bigm| \bigg\} \Big."),
        [
            Node::Big {
                delim: Some('('),
                size: 2,
                class: Class::Open
            },
            Node::Big {
                delim: Some('|'),
                size: 1,
                class: Class::Rel
            },
            Node::Big {
                delim: Some('}'),
                size: 3,
                class: Class::Ord
            },
            Node::Big {
                delim: None,
                size: 2,
                class: Class::Ord
            },
        ]
    );
}

#[quench_test::test]
fn accents_braces_and_stacks() {
    let [
        Node::Accent {
            accent: Accent::WideHat,
            ..
        },
    ] = &body(r"\widehat{xy}")[..]
    else {
        panic!()
    };
    assert!(Accent::WideHat.is_wide() && !Accent::Hat.is_wide());
    let [
        Node::Brace {
            over: false,
            label: Some(label),
            ..
        },
    ] = &body(r"\underbrace{a+b}_{n}")[..]
    else {
        panic!()
    };
    assert_eq!(label, &[ch('n', Class::Ord)]);
    let [
        Node::Stack {
            class: Class::Rel,
            over: Some(_),
            under: None,
            ..
        },
    ] = &body(r"\stackrel{?}{=}")[..]
    else {
        panic!()
    };
    let [
        Node::Stack {
            class: Class::Ord,
            over: None,
            under: Some(_),
            ..
        },
    ] = &body(r"\underset{x}{y}")[..]
    else {
        panic!()
    };
    assert!(matches!(&body(r"\overline{x}")[..], [Node::Overline(_)]));
}

#[quench_test::test]
fn fonts_text_styles_and_colours_are_kept() {
    assert!(matches!(
        &body(r"\mathbb{R}")[..],
        [Node::Font {
            font: MathFont::Blackboard,
            ..
        }]
    ));
    assert!(matches!(
        &body(r"\bf x")[..],
        [Node::Font {
            font: MathFont::Bold,
            ..
        }]
    ));
    let [
        Node::Text {
            font: TextFont::Normal,
            body: parts,
        },
    ] = &body(r"\text{for  $x$ \textbf{all}}")[..]
    else {
        panic!()
    };
    assert_eq!(parts[0], TextPart::Str("for ".into()));
    assert!(matches!(&parts[1], TextPart::Math(math) if math.len() == 1));
    assert_eq!(parts[2], TextPart::Str(" ".into()));
    assert_eq!(
        parts[3],
        TextPart::Styled(TextFont::Bold, vec![TextPart::Str("all".into())])
    );
    let [
        Node::Char(_),
        Node::Style {
            style: Style::Display,
            body: rest,
        },
    ] = &body(r"a \displaystyle b c")[..]
    else {
        panic!()
    };
    assert_eq!(rest.len(), 2);
    let [
        Node::Color {
            color,
            body: coloured,
        },
    ] = &body(r"\color{#ff0000} x")[..]
    else {
        panic!()
    };
    assert_eq!((color.as_str(), coloured.len()), ("#ff0000", 1));
    let [Node::Color { color, .. }] = &body(r"\textcolor[rgb]{1,0,0}{x}")[..] else {
        panic!()
    };
    assert_eq!(color, "rgb:1,0,0");
}

#[quench_test::test]
fn spacing_is_recorded_in_its_unit() {
    let spaces: Vec<Space> = body(r"\, \: \; \! \quad \qquad \hspace{2em} \mkern3mu \hspace{5pt}")
        .into_iter()
        .map(|n| {
            if let Node::Space(s) = n {
                s
            } else {
                panic!("{n:?}")
            }
        })
        .collect();
    assert_eq!(
        spaces,
        [
            Space::Mu(3.0),
            Space::Mu(4.0),
            Space::Mu(5.0),
            Space::Mu(-3.0),
            Space::Em(1.0),
            Space::Em(2.0),
            Space::Em(2.0),
            Space::Mu(3.0),
            Space::Em(0.5)
        ]
    );
    assert!((Space::Mu(9.0).em() - 0.5).abs() < 1e-6);
}

#[quench_test::test]
fn arrays_have_columns_rows_rules_and_delimiters() {
    let [Node::Array(array)] = &body(r"\begin{array}{l|cr} a & b & c \\ \hline d \end{array}")[..]
    else {
        panic!()
    };
    assert_eq!(array.kind, ArrayKind::Array);
    assert_eq!(
        array.columns,
        [
            Column::Align(Align::Left),
            Column::Rule,
            Column::Align(Align::Center),
            Column::Align(Align::Right)
        ]
    );
    assert_eq!(
        (array.rows.len(), array.rows[0].len(), array.rows[1].len()),
        (2, 3, 1)
    );
    assert_eq!(array.hlines, [1]);
    let [Node::Array(matrix)] = &body(r"\begin{pmatrix} 1 & 2 \\ 3 & 4 \\ \end{pmatrix}")[..]
    else {
        panic!()
    };
    assert_eq!(
        (matrix.open, matrix.close, matrix.rows.len()),
        (Some('('), Some(')'), 2),
        "a trailing \\\\ adds no row"
    );
    assert_eq!(matrix.columns, [Column::Align(Align::Center); 2]);
    let [Node::Array(aligned)] = &body(r"\begin{align*} a &= b & c &= d \end{align*}")[..] else {
        panic!()
    };
    assert_eq!(aligned.kind, ArrayKind::Aligned);
    let aligns = [Align::Right, Align::Left, Align::Right, Align::Left].map(Column::Align);
    assert_eq!(aligned.columns, aligns);
    let [Node::Array(cases)] = &body(r"\begin{cases} 1 & x \end{cases}")[..] else {
        panic!()
    };
    assert_eq!(
        (cases.kind, cases.open, cases.close),
        (ArrayKind::Cases, Some('{'), None)
    );
}

#[quench_test::test]
fn mathematical_alphanumerics_fill_the_holes() {
    assert_eq!(math_alphanumeric('R', MathFont::Blackboard), 'ℝ');
    assert_eq!(math_alphanumeric('S', MathFont::Blackboard), '𝕊');
    assert_eq!(math_alphanumeric('e', MathFont::Calligraphic), 'ℯ');
    assert_eq!(math_alphanumeric('Z', MathFont::Fraktur), 'ℨ');
    assert_eq!(math_alphanumeric('Ω', MathFont::Bold), '𝛀');
    assert_eq!(math_alphanumeric('7', MathFont::Monospace), '𝟽');
    assert_eq!(math_alphanumeric('x', MathFont::Roman), 'x');
    assert_eq!(math_alphanumeric('+', MathFont::Blackboard), '+');
}

#[quench_test::test]
fn display_width_counts_terminal_cells() {
    assert_eq!(display_width("abc"), 3);
    assert_eq!(display_width("x\u{302}y\u{305}"), 2);
    assert_eq!(display_width("∑ᵢ"), 2);
    assert_eq!(display_width("数学"), 4);
    assert_eq!(display_width(""), 0);
}

#[quench_test::test]
fn display_width_counts_emoji_presentation() {
    assert_eq!(display_width("\u{2764}"), 1);
    assert_eq!(display_width("\u{2764}\u{FE0F}"), 2);
    assert_eq!(display_width("#\u{FE0F}\u{20E3}"), 2);
    assert_eq!(display_width("\u{263A}\u{FE0E}"), 1);
    assert_eq!(display_width(" \u{FE0F}"), 1);
}

#[quench_test::test]
fn display_width_counts_zwj_sequences_as_one_emoji() {
    assert_eq!(display_width("👨\u{200D}👩\u{200D}👧"), 2);
    assert_eq!(display_width("👨\u{200D}👩\u{200D}👧\u{200D}👦 x"), 4);
    assert_eq!(display_width("🏃\u{200D}\u{2640}\u{FE0F}"), 2);
    assert_eq!(display_width("🏳\u{FE0F}\u{200D}🌈"), 2);
    // A joiner outside an emoji sequence joins nothing.
    assert_eq!(display_width("a\u{200D}b"), 2);
    assert_eq!(display_width("😀\u{200D} x"), 4);
}

#[quench_test::test]
fn display_width_gives_skin_tones_no_cell() {
    assert_eq!(display_width("👍🏽"), 2);
    assert_eq!(display_width("👩🏾\u{200D}💻"), 2);
    assert_eq!(display_width("\u{1F3FB}"), 0);
}

#[quench_test::test]
fn display_width_counts_a_flag_as_two() {
    assert_eq!(display_width("🇵🇱"), 2);
    assert_eq!(display_width("🇵🇱🇩🇪"), 4);
    assert_eq!(display_width("🇵"), 1);
    assert_eq!(display_width("🇵🇱🇩"), 3);
}
