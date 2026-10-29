// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::unwrap_used, clippy::too_many_lines, clippy::float_cmp)]

use std::rc::Rc;

use quench_css::{
    Align, BackgroundSize, BorderStyle, Cascade, Color, ColorValue, ComputedStyle, Display,
    Element, Justify, Length, MediaEnv, Position, Simple, State, Stylesheet, Token, Value, accepts,
    parse, parse_selector_list, parse_values, tokenize,
};
use quench_css::{
    ColorScheme, Direction, FillMode, GridLine, IGNORED, LineHeight, Pointer, SUPPORTED,
    TextTransform, Timing, Track,
};

/// A small tree to match selectors against.
struct Tree {
    nodes: Vec<Data>,
}

struct Data {
    tag: &'static str,
    id: Option<&'static str>,
    classes: Vec<&'static str>,
    attrs: Vec<(&'static str, &'static str)>,
    parent: Option<usize>,
    hover: bool,
}

#[derive(Clone)]
struct Node {
    tree: Rc<Tree>,
    at: usize,
}

impl Node {
    fn child_of(&self) -> Option<usize> {
        self.tree.nodes[self.at].parent
    }
}

impl Element for Node {
    fn tag(&self) -> String {
        self.tree.nodes[self.at].tag.to_string()
    }
    fn id(&self) -> Option<String> {
        self.tree.nodes[self.at].id.map(String::from)
    }
    fn has_class(&self, class: &str) -> bool {
        self.tree.nodes[self.at].classes.contains(&class)
    }
    fn attribute(&self, name: &str) -> Option<String> {
        self.tree.nodes[self.at]
            .attrs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| (*v).to_string())
    }
    fn parent(&self) -> Option<Self> {
        self.child_of().map(|at| Self {
            tree: Rc::clone(&self.tree),
            at,
        })
    }
    fn previous_sibling(&self) -> Option<Self> {
        let parent = self.child_of();
        (0..self.at)
            .rev()
            .find(|i| self.tree.nodes[*i].parent == parent)
            .map(|at| Self {
                tree: Rc::clone(&self.tree),
                at,
            })
    }
    fn state(&self, state: State) -> bool {
        state == State::Hover && self.tree.nodes[self.at].hover
    }
}

const fn data(tag: &'static str, parent: Option<usize>) -> Data {
    Data {
        tag,
        id: None,
        classes: Vec::new(),
        attrs: Vec::new(),
        parent,
        hover: false,
    }
}

/// `html > body > (div#main.card > (p.lead, button[aria-checked=true]))`.
fn sample() -> Vec<Node> {
    let mut nodes = vec![
        data("html", None),
        data("body", Some(0)),
        data("div", Some(1)),
        data("p", Some(2)),
        data("button", Some(2)),
    ];
    nodes[2].id = Some("main");
    nodes[2].classes = vec!["card"];
    nodes[3].classes = vec!["lead"];
    nodes[4].attrs = vec![("aria-checked", "true")];
    let tree = Rc::new(Tree { nodes });
    (0..5)
        .map(|at| Node {
            tree: Rc::clone(&tree),
            at,
        })
        .collect()
}

fn selector(text: &str) -> quench_css::Complex {
    let mut list = parse_selector_list(&tokenize(text));
    assert_eq!(list.len(), 1, "{text}");
    list.remove(0)
}

fn style_of(css: &str, element: usize, media: MediaEnv) -> ComputedStyle {
    let nodes = sample();
    let mut cascade = Cascade::new(media);
    cascade.add(&parse(css));
    // Compute down from the root so inheritance is real.
    let mut chain = Vec::new();
    let mut at = Some(nodes[element].clone());
    while let Some(n) = at {
        at = n.parent();
        chain.push(n);
    }
    let mut style: Option<ComputedStyle> = None;
    for n in chain.iter().rev() {
        style = Some(cascade.compute(n, style.as_ref(), 16.0));
    }
    style.unwrap()
}

fn style(css: &str, element: usize) -> ComputedStyle {
    style_of(css, element, MediaEnv::default())
}

#[quench_test::test]
fn the_tokenizer_reads_every_kind_of_token() {
    let tokens = tokenize(
        "a.b #c { margin: -1.5px 50% rgb(0 0 0 / .5); content: \"x\"; } @media /* c */ (a)",
    );
    assert!(tokens.contains(&Token::Hash("c".into())));
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t, Token::Number { value, unit } if *value == -1.5 && unit == "px"))
    );
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t, Token::Number { value, unit } if *value == 50.0 && unit == "%"))
    );
    assert!(
        tokens
            .iter()
            .any(|t| matches!(t, Token::Function(n) if n == "rgb"))
    );
    assert!(tokens.contains(&Token::Str("x".into())));
    assert!(tokens.contains(&Token::AtKeyword("media".into())));
    let values = parse_values(&tokenize("1px solid var(--a, red)"));
    assert_eq!(values.len(), 3);
    assert!(
        matches!(&values[2], Value::Function(n, args) if n == "var" && args.contains(&Value::Comma))
    );
}

#[quench_test::test]
fn specificity_counts_ids_classes_and_tags() {
    assert_eq!(selector("p").specificity(), 1);
    assert_eq!(selector(".a.b").specificity(), 2 << 8);
    assert_eq!(selector("#x .a p").specificity(), (1 << 16) | (1 << 8) | 1);
    assert_eq!(selector(":not(.a)").specificity(), 1 << 8);
    assert_eq!(selector("*").specificity(), 0);
}

#[quench_test::test]
fn selectors_match_by_tag_class_id_attribute_and_relation() {
    let n = sample();
    assert!(selector("p").matches(&n[3]));
    assert!(selector(".lead").matches(&n[3]));
    assert!(selector("div#main.card > p.lead").matches(&n[3]));
    assert!(selector("body p").matches(&n[3]));
    assert!(!selector("body > p").matches(&n[3]));
    assert!(selector("p + button").matches(&n[4]));
    assert!(!selector("button + p").matches(&n[3]));
    assert!(selector("[aria-checked=\"true\"]").matches(&n[4]));
    assert!(selector("[aria-checked]").matches(&n[4]));
    assert!(!selector("[aria-checked=\"false\"]").matches(&n[4]));
    assert!(selector("button:not([class])").matches(&n[4]));
    assert!(!selector("p:not(.lead)").matches(&n[3]));
    assert!(selector(":root").matches(&n[0]));
    assert!(!selector(":root").matches(&n[1]));
    assert!(selector("p:first-child").matches(&n[3]));
    assert!(!selector("button:first-child").matches(&n[4]));
    assert!(selector("*").matches(&n[2]));
    assert!(
        !selector("p::before").matches(&n[3]),
        "pseudo-elements never match"
    );
    assert!(!selector("p:unheard-of").matches(&n[3]));
    assert!(matches!(
        selector("p::before").parts[0].1.conditions[0],
        Simple::Unknown
    ));
}

#[quench_test::test]
fn interaction_states_come_from_the_tree() {
    let mut nodes = sample();
    let hovered = {
        let mut tree = Tree {
            nodes: vec![data("a", None)],
        };
        tree.nodes[0].hover = true;
        Node {
            tree: Rc::new(tree),
            at: 0,
        }
    };
    assert!(selector("a:hover").matches(&hovered));
    assert!(!selector("a:focus").matches(&hovered));
    assert!(!selector("p:hover").matches(&nodes.remove(3)));
}

#[quench_test::test]
fn nesting_with_and_without_the_ampersand() {
    let css = ".card { color: red; & .lead { color: blue; } .lead + button { color: green; } &:hover { color: gold; } p { margin: 1px; } }";
    let sheet = parse(css);
    let selectors: Vec<usize> = sheet.rules.iter().map(|r| r.selector.parts.len()).collect();
    assert_eq!(sheet.rules.len(), 5, "{selectors:?}");
    assert_eq!(style(css, 3).color, Color::rgba(0, 0, 255, 255));
    assert_eq!(style(css, 2).color, Color::rgba(255, 0, 0, 255));
    assert_eq!(style(css, 3).margin[0].as_ref().unwrap().as_px(), Some(1.0));
    // A nested rule that starts with a combinator keeps it.
    let css = ".card { > p { color: blue } }";
    assert_eq!(style(css, 3).color, Color::rgba(0, 0, 255, 255));
    assert_eq!(style(css, 4).color, Color::BLACK);
}

#[quench_test::test]
fn the_cascade_orders_by_importance_specificity_then_position() {
    assert_eq!(
        style("p { color: red } p { color: blue }", 3).color,
        Color::rgba(0, 0, 255, 255),
        "later wins"
    );
    assert_eq!(
        style(".lead { color: red } p { color: blue }", 3).color,
        Color::rgba(255, 0, 0, 255),
        "specific wins"
    );
    assert_eq!(
        style("p { color: red !important } #main p { color: blue }", 3).color,
        Color::rgba(255, 0, 0, 255),
        "important wins"
    );
    assert_eq!(
        style(
            "p { color: red !important } p { color: blue !important }",
            3
        )
        .color,
        Color::rgba(0, 0, 255, 255),
        "and then the rest decides"
    );
}

#[quench_test::test]
fn inheritance_covers_text_but_not_boxes() {
    let s = style(
        "body { color: red; font-size: 20px; padding: 4px; background: blue; text-transform: uppercase } p { }",
        3,
    );
    assert_eq!(s.color, Color::rgba(255, 0, 0, 255));
    assert_eq!(s.font_size, 20.0);
    assert_eq!(s.text_transform, TextTransform::Uppercase);
    assert_eq!(s.padding[0].as_px(), Some(0.0), "padding does not inherit");
    assert_eq!(s.background, ColorValue::Color(Color::TRANSPARENT));
    let s = style(
        "body { color: red } p { color: inherit } div { color: blue }",
        3,
    );
    assert_eq!(
        s.color,
        Color::rgba(0, 0, 255, 255),
        "inherit takes the parent's, which is blue"
    );
    let s = style(
        "body { font-size: 10px; line-height: 1.5 } p { font: inherit }",
        3,
    );
    assert_eq!(
        (s.font_size, s.line_height),
        (10.0, LineHeight::Number(1.5))
    );
}

#[quench_test::test]
fn custom_properties_inherit_and_fall_back() {
    let css = ":root { --c: red; --d: var(--c); --gap: 8px } body { --c: blue } p { color: var(--c); margin: var(--gap) var(--missing, 3px) } button { color: var(--nope) } div { color: var(--d) }";
    let p = style(css, 3);
    assert_eq!(
        p.color,
        Color::rgba(0, 0, 255, 255),
        "the nearest declaration"
    );
    assert_eq!(p.margin[0].as_ref().unwrap().as_px(), Some(8.0));
    assert_eq!(
        p.margin[1].as_ref().unwrap().as_px(),
        Some(3.0),
        "the fallback"
    );
    assert_eq!(
        style(css, 4).color,
        Color::rgba(255, 0, 0, 255),
        "no value and no fallback: the declaration is dropped, the inherited colour stays"
    );
    // `--d` was resolved where it was declared, so `body`'s `--c` does not reach it.
    assert_eq!(style(css, 2).color, Color::rgba(255, 0, 0, 255));
    let cycle = ":root { --a: var(--b); --b: var(--a) } p { color: var(--a, green) }";
    assert_eq!(
        style(cycle, 3).color,
        Color::rgba(0, 128, 0, 255),
        "a cycle is no value, so the fallback is used"
    );
}

#[quench_test::test]
fn units_and_calc_resolve_against_the_right_things() {
    let s = style(
        "body { font-size: 20px } p { width: 2em; height: 2rem; padding: 1vw 1vh 1dvh 0; margin-left: calc(10px + 2 * 3px) }",
        3,
    );
    assert_eq!(s.width.as_ref().unwrap().as_px(), Some(40.0));
    assert_eq!(s.height.as_ref().unwrap().as_px(), Some(32.0));
    assert_eq!(s.padding[0].as_px(), Some(12.8));
    assert_eq!(s.padding[1].as_px(), Some(8.0));
    assert_eq!(s.padding[2].as_px(), Some(8.0));
    assert_eq!(s.margin[3].as_ref().unwrap().as_px(), Some(16.0));
    let s = style(
        "p { width: calc(100% - 20px); height: min(50%, 100px); max-width: max(10px, 20px); min-height: clamp(1px, 5px, 3px) }",
        3,
    );
    let width = s.width.unwrap();
    assert_eq!(width.resolve(200.0), 180.0);
    assert!(width.is_relative());
    assert_eq!(s.height.unwrap().resolve(100.0), 50.0);
    assert_eq!(s.max_width.unwrap().as_px(), Some(20.0));
    assert_eq!(s.min_height.unwrap().as_px(), Some(3.0));
    let s = style(
        "body { font-size: 10px } p { font-size: 2em; width: 1em }",
        3,
    );
    assert_eq!(
        (s.font_size, s.width.unwrap().as_px()),
        (20.0, Some(20.0)),
        "em in font-size is the parent's, elsewhere the element's"
    );
}

#[quench_test::test]
fn colours_in_every_notation() {
    let color = |value: &str| style(&format!("p {{ color: {value} }}"), 3).color;
    assert_eq!(color("#fff"), Color::WHITE);
    assert_eq!(color("#00ff0080"), Color::rgba(0, 255, 0, 128));
    assert_eq!(color("rgb(1, 2, 3)"), Color::rgba(1, 2, 3, 255));
    assert_eq!(color("rgb(0 0 0 / 0.5)"), Color::rgba(0, 0, 0, 128));
    assert_eq!(color("rgba(255, 0, 0, 50%)"), Color::rgba(255, 0, 0, 128));
    assert_eq!(color("hsl(120 100% 50%)"), Color::rgba(0, 255, 0, 255));
    assert_eq!(color("white"), Color::WHITE);
    assert_eq!(color("transparent"), Color::TRANSPARENT);
    assert_eq!(
        color("color-mix(in srgb, #000000 25%, #ffffff)"),
        Color::rgba(191, 191, 191, 255)
    );
    assert_eq!(
        color("color-mix(in srgb, red 50%, blue 50%)"),
        Color::rgba(128, 0, 128, 255)
    );
    assert_eq!(color("not-a-colour"), Color::BLACK);
    let s = style(
        "p { color: red; border: 2px solid; background: currentcolor }",
        3,
    );
    assert_eq!(s.resolve(s.border_color[0]), Color::rgba(255, 0, 0, 255));
    assert_eq!(s.resolve(s.background), Color::rgba(255, 0, 0, 255));
}

#[quench_test::test]
fn shorthands_expand_to_their_longhands() {
    let s = style(
        "p { margin: 1px 2px 3px; border: 2px dashed red; border-left-width: 5px; border-radius: 4px 8px / 1px; flex: 1 0 auto; gap: 4px 6px; inset: 0 0 0 auto; overflow: hidden auto; outline: 2px solid blue; outline-offset: 3px }",
        3,
    );
    let px = |l: &Option<Length>| l.as_ref().unwrap().as_px();
    assert_eq!(
        s.margin.iter().map(px).collect::<Vec<_>>(),
        [Some(1.0), Some(2.0), Some(3.0), Some(2.0)]
    );
    assert_eq!(s.border_width, [2.0, 2.0, 2.0, 5.0]);
    assert_eq!(s.border_style[0], BorderStyle::Dashed);
    assert_eq!(s.radius[0].as_px(), Some(4.0));
    assert_eq!(s.radius[1].as_px(), Some(8.0));
    assert_eq!(s.radius[2].as_px(), Some(4.0));
    assert_eq!((s.flex_grow, s.flex_shrink, s.flex_basis), (1.0, 0.0, None));
    assert_eq!(
        (s.row_gap.as_px(), s.column_gap.as_px()),
        (Some(4.0), Some(6.0))
    );
    assert_eq!(s.inset[0].as_ref().unwrap().as_px(), Some(0.0));
    assert!(s.inset[3].is_none(), "auto");
    assert_eq!((s.outline_width, s.outline_offset), (2.0, 3.0));
    let s = style("p { flex: 2 }", 3);
    assert_eq!(
        (s.flex_grow, s.flex_shrink, s.flex_basis.unwrap().as_px()),
        (2.0, 1.0, Some(0.0))
    );
    let s = style("p { flex: none }", 3);
    assert_eq!((s.flex_grow, s.flex_shrink), (0.0, 0.0));
    let s = style("p { border-width: 3px }", 3);
    assert_eq!(s.border_width, [0.0; 4], "no style, no border");
    let s = style(
        "p { font: italic 700 18px/1.2 \"Noto Sans\", sans-serif }",
        3,
    );
    assert_eq!(
        (s.italic, s.font_weight, s.font_size, s.line_height),
        (true, 700, 18.0, LineHeight::Number(1.2))
    );
    assert_eq!(*s.font_family, ["Noto Sans", "sans-serif"]);
}

#[quench_test::test]
fn display_position_flex_and_alignment() {
    let s = style(
        "p { display: flex; position: absolute; flex-direction: column; align-items: center; justify-content: space-between; z-index: 3 }",
        3,
    );
    assert_eq!(
        (
            s.display,
            s.position,
            s.align_items,
            s.justify_content,
            s.z_index
        ),
        (
            Display::Flex,
            Position::Absolute,
            Align::Center,
            Justify::SpaceBetween,
            Some(3)
        )
    );
    let s = style("p { display: inline-block; position: fixed }", 3);
    assert_eq!(s.display, Display::Block, "out of flow boxes are blocks");
    assert_eq!(style("p { display: none }", 3).display, Display::None);
}

#[quench_test::test]
fn shadows_transforms_and_opacity() {
    let s = style(
        "p { box-shadow: 0 1px 2px rgb(0 0 0 / 0.2), inset 0 0 0 1px red; opacity: 0.5; transform: translate(-50%, 4px) scale(0.5); rotate: 90deg }",
        3,
    );
    assert_eq!(s.shadows.len(), 2);
    assert_eq!(
        (s.shadows[0].y, s.shadows[0].blur, s.shadows[0].inset),
        (1.0, 2.0, false)
    );
    assert!(s.shadows[1].inset);
    assert_eq!(s.opacity, 0.5);
    assert_eq!(s.transform.scale, (0.5, 0.5));
    assert!(s.transform.translate.0.is_relative());
    assert_eq!(s.transform.rotate, 90.0);
    assert!(!s.transform.is_identity());
    assert!(style("p { transform: none }", 3).transform.is_identity());
}

#[quench_test::test]
fn media_queries_follow_the_environment() {
    let css = "p { color: red } @media (prefers-color-scheme: light) { p { color: blue } } @media (max-width: 600px) { p { color: green } } @media (hover: none) and (pointer: coarse) { p { color: gold } }";
    let dark = style(css, 3);
    assert_eq!(dark.color, Color::rgba(255, 0, 0, 255));
    let light = style_of(
        css,
        3,
        MediaEnv {
            color_scheme: ColorScheme::Light,
            ..MediaEnv::default()
        },
    );
    assert_eq!(light.color, Color::rgba(0, 0, 255, 255));
    let phone = style_of(
        css,
        3,
        MediaEnv {
            width: 400.0,
            ..MediaEnv::default()
        },
    );
    assert_eq!(phone.color, Color::rgba(0, 128, 0, 255));
    let touch = style_of(
        css,
        3,
        MediaEnv {
            hover: false,
            pointer: Pointer::Coarse,
            ..MediaEnv::default()
        },
    );
    assert_eq!(touch.color, Color::rgba(255, 215, 0, 255));
    let range = "@media (width >= 500px) and (width < 900px) { p { color: red } } @media not (min-width: 100px) { p { color: blue } } @media (prefers-reduced-motion: reduce), (orientation: portrait) { p { margin: 9px } }";
    let at = |width, height| {
        style_of(
            range,
            3,
            MediaEnv {
                width,
                height,
                ..MediaEnv::default()
            },
        )
    };
    assert_eq!(at(600.0, 400.0).color, Color::rgba(255, 0, 0, 255));
    assert_eq!(at(1000.0, 400.0).color, Color::BLACK);
    assert_eq!(
        at(600.0, 900.0).margin[0].as_ref().unwrap().as_px(),
        Some(9.0),
        "portrait"
    );
    assert_eq!(
        at(600.0, 400.0).margin[0].as_ref().unwrap().as_px(),
        Some(0.0)
    );
}

#[quench_test::test]
fn supports_is_evaluated_and_unknown_at_rules_are_recorded() {
    let sheet = parse(
        "@supports (height: 100dvh) { p { height: 100dvh } } @supports (frobnicate: yes) { p { color: red } } @supports not (frobnicate: yes) { p { color: blue } } @charset \"utf-8\"; @font-face { font-family: x }",
    );
    let colors: Vec<_> = sheet
        .rules
        .iter()
        .flat_map(|r| r.declarations.iter().map(|d| d.name.clone()))
        .collect();
    assert_eq!(colors, ["height", "color"], "{colors:?}");
    assert!(
        sheet.ignored.iter().any(|i| i.contains("font-face")),
        "{:?}",
        sheet.ignored
    );
}

#[quench_test::test]
fn keyframes_are_kept_with_their_frames() {
    let sheet = parse(
        "@keyframes spin { from { transform: rotate(0deg) } 50% { opacity: 0.5 } to { transform: rotate(360deg) } }",
    );
    assert_eq!(sheet.keyframes.len(), 1);
    assert_eq!(sheet.keyframes[0].name, "spin");
    let at: Vec<f32> = sheet.keyframes[0].frames.iter().map(|f| f.0).collect();
    assert_eq!(at, [0.0, 0.5, 1.0]);
}

#[quench_test::test]
fn broken_css_is_skipped_not_fatal() {
    let sheet = parse("p { color: red; bogus; width: ; : 3; } } q { color: blue } /* open");
    assert!(
        sheet
            .rules
            .iter()
            .any(|r| r.declarations.iter().any(|d| d.name == "color"))
    );
    let s = style(
        "p { color: red; width: 10px; width: bad value here; height: 4px }",
        3,
    );
    assert_eq!(
        s.width.unwrap().as_px(),
        Some(10.0),
        "an invalid value leaves the earlier one"
    );
    assert_eq!(s.height.unwrap().as_px(), Some(4.0));
    assert!(parse("").rules.is_empty());
    assert!(parse_selector_list(&tokenize("")).is_empty());
}

#[quench_test::test]
fn env_is_the_safe_area() {
    let nodes = sample();
    let mut cascade = Cascade::new(MediaEnv::default());
    cascade.add(&parse("p { padding-top: env(safe-area-inset-top); padding-left: env(safe-area-inset-left, 7px); margin-top: env(nonsense, 5px) }"));
    assert_eq!(
        cascade.compute(&nodes[3], None, 16.0).padding[0].as_px(),
        Some(0.0)
    );
    cascade.safe_area = [24.0, 0.0, 16.0, 3.0];
    let s = cascade.compute(&nodes[3], None, 16.0);
    assert_eq!(
        (s.padding[0].as_px(), s.padding[3].as_px()),
        (Some(24.0), Some(3.0))
    );
    assert_eq!(s.margin[0].as_ref().unwrap().as_px(), Some(5.0));
}

#[quench_test::test]
fn real_stylesheets_use_only_what_the_engine_knows() {
    // Every stylesheet the gallery page loads: a property in neither list, or a value the engine refuses, fails here.
    let sheets: Vec<(&str, String)> = vec![
        ("tokens", quench_style::tokens::stylesheet()),
        ("mobile", quench_style::mobile::stylesheet()),
        ("components", quench_style::components::stylesheet()),
        ("desktop", quench_style::desktop::stylesheet()),
        (
            "dark",
            quench_style::theme::Theme::theme(quench_style::theme::Theme::DefaultDark),
        ),
        (
            "light",
            quench_style::theme::Theme::theme(quench_style::theme::Theme::DefaultLight),
        ),
    ];
    let mut checked = 0;
    for (name, css) in &sheets {
        let sheet: Stylesheet = parse(css);
        assert!(sheet.ignored.is_empty(), "{name}: {:?}", sheet.ignored);
        let all = sheet
            .rules
            .iter()
            .flat_map(|r| r.declarations.iter())
            .chain(
                sheet
                    .keyframes
                    .iter()
                    .flat_map(|k| k.frames.iter().flat_map(|f| f.1.iter())),
            );
        for d in all {
            if d.name.starts_with("--") {
                continue;
            }
            assert!(
                SUPPORTED.contains(&d.name.as_str()) || IGNORED.contains(&d.name.as_str()),
                "{name}: `{}` is not in the subset",
                d.name
            );
            let has_function = format!("{:?}", d.value).contains("\"var\"")
                || format!("{:?}", d.value).contains("\"env\"");
            if SUPPORTED.contains(&d.name.as_str()) && !has_function {
                assert!(
                    accepts(&d.name, &d.value),
                    "{name}: `{}: {:?}` is refused",
                    d.name,
                    d.value
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

#[quench_test::test]
fn real_stylesheets_compute_a_button_that_looks_like_one() {
    // The sheets the page loads, in its order, against a button in the dark scheme.
    let mut cascade = Cascade::new(MediaEnv::default());
    for css in [
        quench_style::theme::Theme::theme(quench_style::theme::Theme::DefaultDark),
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    ] {
        cascade.add(&parse(&css));
    }
    let mut tree = Tree {
        nodes: vec![
            data("html", None),
            data("body", Some(0)),
            data("button", Some(1)),
        ],
    };
    tree.nodes[2].classes = vec!["q-fab"];
    let tree = Rc::new(tree);
    let node = |at| Node {
        tree: Rc::clone(&tree),
        at,
    };
    let html = cascade.compute(&node(0), None, 16.0);
    let body = cascade.compute(&node(1), Some(&html), 16.0);
    let button = cascade.compute(&node(2), Some(&body), 16.0);
    assert!(
        button.background != ColorValue::Color(Color::TRANSPARENT),
        "{:?}",
        button.background
    );
    assert_eq!(button.position, Position::Fixed);
    assert_eq!(
        (
            button.width.as_ref().unwrap().as_px(),
            button.height.as_ref().unwrap().as_px()
        ),
        (Some(56.0), Some(56.0))
    );
    assert_eq!(button.color, Color::WHITE);
    assert_eq!(
        button.inset[1].as_ref().unwrap().as_px(),
        Some(16.0),
        "calc() with env() resolved"
    );
    assert_eq!(button.shadows.len(), 1, "the elevation token through var()");
    assert_eq!(button.border_width, [0.0; 4]);
}

#[quench_test::test]
fn a_style_attribute_wins_over_rules_and_parses_on_its_own() {
    let declarations = quench_css::parse_declarations("color: red; width: 10px !important; bogus");
    assert_eq!(declarations.len(), 2);
    assert!(declarations[1].important);
    let nodes = sample();
    let mut cascade = Cascade::new(MediaEnv::default());
    cascade.add(&parse("#main p { color: blue; margin: 1px }"));
    let inline = quench_css::parse_declarations("color: green");
    let s = cascade.compute_with(&nodes[3], None, 16.0, &inline);
    assert_eq!(
        s.color,
        Color::rgba(0, 128, 0, 255),
        "over a rule with an id in it"
    );
    assert_eq!(
        s.margin[0].as_ref().unwrap().as_px(),
        Some(1.0),
        "the rest still applies"
    );
}

/// The style of the `p` of the sample tree under `decls`.
fn p_with(decls: &str) -> ComputedStyle {
    style(&format!("p {{ {decls} }}"), 3)
}

#[quench_test::test]
fn every_length_unit_and_keyword_resolves() {
    let px = |decl: &str| p_with(decl).width.and_then(|l| l.as_px());
    assert_eq!(px("width: 72pt"), Some(96.0));
    assert_eq!(px("width: 6pc"), Some(96.0));
    assert_eq!(px("width: 1in"), Some(96.0));
    assert_eq!(px("width: 2.54cm").map(f32::round), Some(96.0));
    assert_eq!(px("width: 25.4mm").map(f32::round), Some(96.0));
    assert_eq!(px("width: 101.6q").map(f32::round), Some(96.0));
    assert_eq!(px("width: 10vmin"), Some(80.0));
    assert_eq!(px("width: 10vmax"), Some(128.0));
    assert_eq!(px("width: 10svh"), Some(80.0));
    assert_eq!(px("width: 2ch"), Some(16.0));
    assert_eq!(px("width: 2ex"), Some(16.0));
    assert_eq!(px("width: 0"), Some(0.0));
    assert_eq!(
        px("width: 12furlongs"),
        None,
        "an unknown unit is no length"
    );
    assert_eq!(px("width: calc(2 * (3 + 4) * 1px)"), Some(14.0));
    assert_eq!(px("width: calc(10px / 4)"), Some(2.5));
    assert_eq!(px("width: calc(10px / 0)"), None);
    assert_eq!(px("width: calc(2)"), None, "a number is not a length");
    assert_eq!(px("width: calc(10px + )"), None);
    assert_eq!(px("width: min(10px, 2px, 30px)"), Some(2.0));
    assert_eq!(px("width: max(10px, 2px, 30px)"), Some(30.0));
    assert_eq!(px("width: clamp(10px, 50px, 30px)"), Some(30.0));
    assert_eq!(px("width: clamp(10px, 20px)"), None);
    assert_eq!(px("width: calc(min(10px, 5px) + max(1px, 2px))"), Some(7.0));
    assert_eq!(
        px("width: calc(100% - 10px)"),
        None,
        "a percentage waits for layout"
    );
    assert!(
        p_with("width: calc(100% - 10px)")
            .width
            .unwrap()
            .is_relative()
    );
    assert!(p_with("width: max(10%, 5px)").width.unwrap().is_relative());
    assert_eq!(
        p_with("width: min(50%, 80px)")
            .width
            .unwrap()
            .resolve(100.0),
        50.0
    );
    assert_eq!(
        p_with("width: calc(100% - 10px)")
            .width
            .unwrap()
            .resolve(50.0),
        40.0
    );
    assert_eq!(
        p_with("width: calc(50% * 2)").width.unwrap().resolve(30.0),
        30.0
    );
    assert_eq!(
        p_with("width: calc(2 * 50%)").width.unwrap().resolve(30.0),
        30.0
    );
    assert_eq!(
        p_with("width: calc(100% / 4)").width.unwrap().resolve(40.0),
        10.0
    );
    assert_eq!(
        p_with("width: calc(100% - (10% + 5px))")
            .width
            .unwrap()
            .resolve(100.0),
        85.0
    );
    assert!(p_with("width: auto").width.is_none());
    assert!(p_with("width: 5px; width: auto").width.is_none());
}

#[quench_test::test]
fn colour_functions_and_failures() {
    let color = |value: &str| p_with(&format!("color: {value}")).color;
    assert_eq!(color("#abcd"), Color::rgba(170, 187, 204, 221));
    assert_eq!(color("#zzz"), Color::BLACK);
    assert_eq!(color("#12345"), Color::BLACK);
    assert_eq!(color("rgb(10%, 20%, 30%)"), Color::rgba(26, 51, 77, 255));
    assert_eq!(
        color("rgb(300 0 0)"),
        Color::rgba(255, 0, 0, 255),
        "channels are clamped"
    );
    assert_eq!(color("rgb(1 2)"), Color::BLACK);
    assert_eq!(color("rgb(1 2 3 / 2)"), Color::rgba(1, 2, 3, 255));
    assert_eq!(color("rgb(1 2 3 / x)"), Color::BLACK);
    assert_eq!(color("hsl(0 100% 50%)"), Color::rgba(255, 0, 0, 255));
    assert_eq!(color("hsl(240deg, 100%, 50%)"), Color::rgba(0, 0, 255, 255));
    assert_eq!(
        color("hsla(120, 100%, 25%, 0.5)"),
        Color::rgba(0, 128, 0, 128)
    );
    assert_eq!(color("hsl(0 100)"), Color::BLACK);
    assert_eq!(color("hsl(red 100% 50%)"), Color::BLACK);
    assert_eq!(
        color("color-mix(in srgb, red, blue)"),
        Color::rgba(128, 0, 128, 255)
    );
    assert_eq!(
        color("color-mix(in srgb, red 25%, blue)"),
        Color::rgba(64, 0, 191, 255)
    );
    assert_eq!(
        color("color-mix(in srgb, red, blue 25%)"),
        Color::rgba(191, 0, 64, 255)
    );
    assert_eq!(color("color-mix(in srgb, red 0%, blue 0%)"), Color::BLACK);
    assert_eq!(color("color-mix(red, blue)"), Color::BLACK);
    assert_eq!(
        color("color-mix(in srgb, currentcolor, blue)"),
        Color::BLACK
    );
    assert_eq!(
        color("RED"),
        Color::rgba(255, 0, 0, 255),
        "names are case-insensitive"
    );
    assert_eq!(color("rebeccapurple"), Color::rgba(102, 51, 153, 255));
    assert_eq!(color("42"), Color::BLACK);
    assert_eq!(Color::rgba(10, 20, 30, 200).with_opacity(0.5).a, 100);
    assert_eq!(
        quench_css::ColorValue::CurrentColor.resolve(Color::WHITE),
        Color::WHITE
    );
    let parsed = quench_css::parse_color(&Value::Ident("transparent".into()));
    assert_eq!(
        parsed,
        Some(quench_css::ColorValue::Color(Color::TRANSPARENT))
    );
    assert!(quench_css::parse_color(&Value::Number(1.0)).is_none());
}

#[quench_test::test]
fn longhands_and_keywords_of_every_supported_property() {
    let s = p_with(
        "position: sticky; top: 1px; right: auto; bottom: 3px; left: 4px; z-index: 5; min-width: 6px; max-width: none; min-height: auto; max-height: 7px; box-sizing: border-box",
    );
    assert_eq!(s.position, Position::Relative);
    assert_eq!(s.inset[0].as_ref().unwrap().as_px(), Some(1.0));
    assert!(s.inset[1].is_none());
    assert_eq!(s.z_index, Some(5));
    assert_eq!(s.min_width.unwrap().as_px(), Some(6.0));
    assert!(s.max_width.is_none() && s.min_height.is_none());
    assert_eq!(s.max_height.unwrap().as_px(), Some(7.0));
    assert!(s.border_box);
    assert_eq!(p_with("z-index: auto").z_index, None);
    assert_eq!(p_with("position: static").position, Position::Static);
    for (decl, want) in [
        ("display: block", Display::Block),
        ("display: inline", Display::Inline),
        ("display: inline-block", Display::InlineBlock),
        ("display: inline-flex", Display::InlineFlex),
        ("display: list-item", Display::ListItem),
        ("display: grid", Display::Grid),
        ("display: inline-grid", Display::InlineGrid),
        ("display: contents", Display::Block),
        ("display: none", Display::None),
    ] {
        assert_eq!(p_with(decl).display, want, "{decl}");
    }
    let s = p_with(
        "margin-top: 1px; margin-right: auto; margin-bottom: 3px; margin-left: 4px; padding-top: 1px; padding-right: 2px; padding-bottom: 3px; padding-left: 4px",
    );
    assert_eq!(s.margin[0].as_ref().unwrap().as_px(), Some(1.0));
    assert!(s.margin[1].is_none());
    assert_eq!(
        s.padding
            .iter()
            .map(|l| l.as_px().unwrap())
            .collect::<Vec<_>>(),
        [1.0, 2.0, 3.0, 4.0]
    );
    let s = p_with(
        "border-style: solid dashed dotted none; border-width: 1px 2px 3px 4px; border-color: red blue green white",
    );
    assert_eq!(
        s.border_style,
        [
            BorderStyle::Solid,
            BorderStyle::Dashed,
            BorderStyle::Dotted,
            BorderStyle::None
        ]
    );
    assert_eq!(
        s.border_width,
        [1.0, 2.0, 3.0, 0.0],
        "the left one has no style"
    );
    let s = p_with(
        "border-top-width: thin; border-top-style: solid; border-right-width: medium; border-right-style: double; border-bottom: thick solid blue; border-left-color: red; border-left-style: solid; border-left-width: 2px",
    );
    assert_eq!(s.border_width, [1.0, 3.0, 5.0, 2.0]);
    assert_eq!(s.border_style[1], BorderStyle::Solid);
    assert_eq!(s.resolve(s.border_color[2]), Color::rgba(0, 0, 255, 255));
    assert_eq!(s.resolve(s.border_color[3]), Color::rgba(255, 0, 0, 255));
    let s = p_with(
        "outline-style: solid; outline-width: 4px; outline-color: red; outline-offset: -2px",
    );
    assert_eq!((s.outline_width, s.outline_offset), (4.0, -2.0));
    assert_eq!(s.resolve(s.outline_color), Color::rgba(255, 0, 0, 255));
    assert_eq!(
        p_with("background: none").background,
        ColorValue::Color(Color::TRANSPARENT)
    );
    assert_eq!(
        p_with("background: url(x.png) no-repeat red").background,
        ColorValue::Color(Color::rgba(255, 0, 0, 255))
    );
    assert_eq!(
        p_with("background-color: blue").background,
        ColorValue::Color(Color::rgba(0, 0, 255, 255))
    );
    assert_eq!(p_with("opacity: 50%").opacity, 0.5);
    assert_eq!(p_with("opacity: 7").opacity, 1.0);
    assert_eq!(p_with("opacity: banana").opacity, 1.0);
    let s = p_with("overflow-x: scroll; overflow-y: clip");
    assert_eq!(
        s.overflow,
        (quench_css::Overflow::Scroll, quench_css::Overflow::Hidden)
    );
    assert_eq!(
        p_with("overflow: auto").overflow,
        (quench_css::Overflow::Auto, quench_css::Overflow::Auto)
    );
    assert_eq!(
        p_with("overflow: visible hidden").overflow,
        (quench_css::Overflow::Visible, quench_css::Overflow::Hidden)
    );
    let s = p_with(
        "flex-direction: column-reverse; flex-wrap: wrap; flex-grow: 2; flex-shrink: 0; flex-basis: 30px; align-self: flex-end; align-items: baseline; row-gap: 3px; column-gap: 4px",
    );
    assert_eq!(s.flex_direction, quench_css::FlexDirection::ColumnReverse);
    assert!(s.flex_wrap);
    assert_eq!((s.flex_grow, s.flex_shrink), (2.0, 0.0));
    assert_eq!(s.flex_basis.unwrap().as_px(), Some(30.0));
    assert_eq!(
        (s.align_self, s.align_items),
        (Some(Align::End), Align::Baseline)
    );
    assert_eq!(
        (s.row_gap.as_px(), s.column_gap.as_px()),
        (Some(3.0), Some(4.0))
    );
    assert_eq!(
        p_with("flex-direction: row-reverse").flex_direction,
        quench_css::FlexDirection::RowReverse
    );
    assert_eq!(p_with("flex-basis: auto").flex_basis, None);
    assert_eq!(p_with("align-self: auto").align_self, None);
    for (decl, want) in [
        ("justify-content: flex-end", Justify::End),
        ("justify-content: center", Justify::Center),
        ("justify-content: space-between", Justify::SpaceBetween),
        ("justify-content: space-around", Justify::SpaceAround),
        ("justify-content: space-evenly", Justify::SpaceEvenly),
        ("justify-content: left", Justify::Start),
    ] {
        assert_eq!(p_with(decl).justify_content, want, "{decl}");
    }
    assert_eq!(p_with("align-items: stretch").align_items, Align::Stretch);
    assert_eq!(
        p_with("flex: 1 2 10px").flex_basis.unwrap().as_px(),
        Some(10.0)
    );
    assert_eq!(
        (
            p_with("flex: auto").flex_grow,
            p_with("flex: initial").flex_grow
        ),
        (1.0, 0.0)
    );
    assert_eq!(p_with("flex: 1 1 auto").flex_basis, None);
    assert_eq!(p_with("flex: bogus").flex_grow, 0.0);
}

#[quench_test::test]
fn text_properties_and_font_shorthand_variants() {
    let s = p_with(
        "font-style: oblique; font-weight: 600; font-size: large; line-height: 24px; text-align: center; text-transform: capitalize; list-style: decimal inside; visibility: collapse; font-family: Inter, \"Noto Sans\", sans-serif",
    );
    assert!(s.italic);
    assert_eq!(
        (s.font_weight, s.font_size, s.line_height),
        (600, 18.0, LineHeight::Px(24.0))
    );
    assert_eq!(s.text_transform, TextTransform::Capitalize);
    assert_eq!(s.list_style, quench_css::ListStyle::Decimal);
    assert!(!s.visible);
    assert_eq!(*s.font_family, ["Inter", "Noto Sans", "sans-serif"]);
    for (decl, size) in [
        ("font-size: xx-small", 9.0),
        ("font-size: x-small", 10.0),
        ("font-size: small", 13.0),
        ("font-size: medium", 16.0),
        ("font-size: x-large", 24.0),
        ("font-size: xx-large", 32.0),
        ("font-size: larger", 19.2),
        ("font-size: 200%", 32.0),
    ] {
        assert!((p_with(decl).font_size - size).abs() < 0.01, "{decl}");
    }
    assert!((p_with("font-size: smaller").font_size - 16.0 / 1.2).abs() < 0.01);
    for (decl, weight) in [
        ("font-weight: normal", 400),
        ("font-weight: bold", 700),
        ("font-weight: 300", 300),
        ("font-weight: bolder", 700),
        ("font-weight: lighter", 100),
    ] {
        assert_eq!(p_with(decl).font_weight, weight, "{decl}");
    }
    assert_eq!(
        style("body { font-weight: 700 } p { font-weight: lighter }", 3).font_weight,
        400
    );
    assert_eq!(
        style("body { font-weight: 300 } p { font-weight: bolder }", 3).font_weight,
        400
    );
    assert_eq!(
        style("body { font-weight: 500 } p { font-weight: bolder }", 3).font_weight,
        700
    );
    assert_eq!(
        style("body { font-weight: 900 } p { font-weight: bolder }", 3).font_weight,
        900
    );
    assert_eq!(
        style("body { font-weight: 500 } p { font-weight: lighter }", 3).font_weight,
        100
    );
    assert_eq!(
        style("body { font-weight: 700 } p { font-weight: inherit }", 3).font_weight,
        700
    );
    assert_eq!(
        p_with("line-height: normal").line_height,
        LineHeight::Normal
    );
    assert_eq!(
        p_with("line-height: 150%").line_height,
        LineHeight::Number(1.5)
    );
    assert_eq!(
        p_with("line-height: 2").line_height,
        LineHeight::Number(2.0)
    );
    assert_eq!(p_with("line-height: -").line_height, LineHeight::Normal);
    for (decl, want) in [
        ("text-align: left", quench_css::TextAlign::Left),
        ("text-align: right", quench_css::TextAlign::Right),
        ("text-align: end", quench_css::TextAlign::End),
        ("text-align: justify", quench_css::TextAlign::Start),
    ] {
        assert_eq!(p_with(decl).text_align, want, "{decl}");
    }
    assert_eq!(
        p_with("text-transform: lowercase").text_transform,
        TextTransform::Lowercase
    );
    assert_eq!(
        p_with("text-transform: none").text_transform,
        TextTransform::None
    );
    assert_eq!(
        p_with("list-style: none").list_style,
        quench_css::ListStyle::None
    );
    assert_eq!(
        p_with("list-style: square").list_style,
        quench_css::ListStyle::Disc
    );
    let s = p_with("font: 14px/20px sans-serif");
    assert_eq!(
        (s.font_size, s.line_height, s.font_weight, s.italic),
        (14.0, LineHeight::Px(20.0), 400, false)
    );
    let s = p_with("font: normal bold 12px Inter");
    assert_eq!((s.font_weight, s.font_size), (700, 12.0));
    assert_eq!(
        p_with("font: bold").font_size,
        16.0,
        "no size: the font is not changed"
    );
    let inherited = style(
        "body { font-family: Inter; font-size: 20px; color: red } p { font-family: inherit; font-size: inherit; color: inherit }",
        3,
    );
    assert_eq!(inherited.font_size, 20.0);
    assert_eq!(*inherited.font_family, ["Inter"]);
    assert_eq!(
        style("body { font-size: 10px } p { font-size: 150% }", 3).font_size,
        15.0
    );
    assert_eq!(
        style(
            "body { font-size: 10px } p { padding: 1em; font-size: 20px }",
            3
        )
        .padding[0]
            .as_px(),
        Some(20.0),
        "em follows the element's own size"
    );
}

#[quench_test::test]
fn transforms_in_all_their_forms() {
    let t = |decl: &str| p_with(decl).transform;
    assert_eq!(
        t("transform: translateX(5px) translateY(7px)")
            .translate
            .0
            .as_px(),
        Some(5.0)
    );
    assert_eq!(
        t("transform: translateX(5px) translateY(7px)")
            .translate
            .1
            .as_px(),
        Some(7.0)
    );
    assert_eq!(
        t("transform: translate(1px, 2px) translate(3px, 4px)")
            .translate
            .0
            .as_px(),
        Some(4.0)
    );
    assert_eq!(
        t("transform: translate(10%) translate(20%)")
            .translate
            .0
            .resolve(100.0),
        30.0
    );
    assert_eq!(t("transform: scale(2, 3)").scale, (2.0, 3.0));
    assert_eq!(t("transform: scaleX(2) scaleY(3)").scale, (2.0, 3.0));
    assert_eq!(t("transform: rotate(0.5turn)").rotate, 180.0);
    assert!((t("transform: rotate(3.14159rad)").rotate - 180.0).abs() < 0.01);
    assert_eq!(t("transform: rotate(100grad)").rotate, 90.0);
    assert_eq!(t("transform: rotate(0)").rotate, 0.0);
    assert!(
        t("transform: skew(10deg)").is_identity(),
        "unknown functions leave it alone"
    );
    assert!(t("transform: translateX(1px) frobnicate(2)").is_identity());
    assert_eq!(t("rotate: 45deg").rotate, 45.0);
    assert_eq!(t("rotate: 45deg; rotate: none").rotate, 0.0);
}

#[quench_test::test]
fn shadows_and_borders_with_odd_values() {
    assert!(p_with("box-shadow: none").shadows.is_empty());
    assert!(
        p_with("box-shadow: 1px").shadows.is_empty(),
        "too few lengths"
    );
    let s = p_with("box-shadow: 1px 2px 3px 4px red");
    assert_eq!(
        (
            s.shadows[0].x,
            s.shadows[0].y,
            s.shadows[0].blur,
            s.shadows[0].spread
        ),
        (1.0, 2.0, 3.0, 4.0)
    );
    assert_eq!(
        p_with("box-shadow: 0 0 5px").shadows[0].color,
        ColorValue::CurrentColor
    );
    assert!(p_with("box-shadow: 1px 2px banana").shadows.is_empty());
    let s = p_with("border: 2px dotted red; border-top: none");
    assert_eq!(s.border_width, [0.0, 2.0, 2.0, 2.0]);
    let s = p_with("border: solid");
    assert_eq!(s.border_width, [3.0; 4], "the initial width is medium");
    assert_eq!(p_with("border: 1px bogus").border_width, [0.0; 4]);
    let s = p_with("border-radius: 10% / 5%; padding: -4px");
    assert!(s.radius[0].is_relative());
    assert_eq!(
        s.padding[0].as_px(),
        Some(0.0),
        "negative padding is invalid"
    );
    let s = p_with("border-radius: 1px 2px 3px");
    assert_eq!(
        s.radius
            .iter()
            .map(|l| l.as_px().unwrap())
            .collect::<Vec<_>>(),
        [1.0, 2.0, 3.0, 2.0]
    );
    assert_eq!(
        p_with("inset: 1px 2px").inset[3].as_ref().unwrap().as_px(),
        Some(2.0)
    );
    assert_eq!(
        p_with("margin: 1px 2px 3px 4px 5px").margin[0]
            .as_ref()
            .unwrap()
            .as_px(),
        Some(0.0),
        "five values are invalid"
    );
    assert_eq!(p_with("gap: 1px 2px 3px").row_gap.as_px(), Some(0.0));
    assert_eq!(
        p_with("overflow: hidden hidden hidden").overflow.0,
        quench_css::Overflow::Visible
    );
    let plain = p_with("");
    assert_eq!(p_with("unheard-of: 3"), plain);
    assert_eq!(
        p_with("cursor: pointer; will-change: opacity; user-select: none"),
        plain,
        "ignored properties do nothing"
    );
}

#[quench_test::test]
fn media_queries_in_their_other_forms() {
    let env = |width: f32, height: f32| MediaEnv {
        width,
        height,
        ..MediaEnv::default()
    };
    let colour = |css: &str, env: MediaEnv| style_of(css, 3, env).color;
    let red = Color::rgba(255, 0, 0, 255);
    assert_eq!(
        colour(
            "@media screen and (min-width: 100px) { p { color: red } }",
            env(200.0, 200.0)
        ),
        red
    );
    assert_eq!(
        colour("@media print { p { color: red } }", env(200.0, 200.0)),
        Color::BLACK
    );
    assert_eq!(
        colour("@media not print { p { color: red } }", env(200.0, 200.0)),
        red
    );
    assert_eq!(
        colour("@media all { p { color: red } }", env(200.0, 200.0)),
        red
    );
    assert_eq!(
        colour(
            "@media (min-height: 100px) and (max-height: 300px) { p { color: red } }",
            env(200.0, 200.0)
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (height > 250px) { p { color: red } }",
            env(200.0, 200.0)
        ),
        Color::BLACK
    );
    assert_eq!(
        colour(
            "@media (100px < width <= 200px) { p { color: red } }",
            env(200.0, 200.0)
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (width = 200px) { p { color: red } }",
            env(200.0, 200.0)
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (orientation: landscape) { p { color: red } }",
            env(300.0, 200.0)
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (orientation: portrait) { p { color: red } }",
            env(300.0, 200.0)
        ),
        Color::BLACK
    );
    assert_eq!(
        colour(
            "@media (prefers-reduced-motion: no-preference) { p { color: red } }",
            env(1.0, 1.0)
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (prefers-reduced-motion: reduce) { p { color: red } }",
            MediaEnv {
                reduced_motion: true,
                ..MediaEnv::default()
            }
        ),
        red
    );
    assert_eq!(
        colour("@media (hover) { p { color: red } }", env(1.0, 1.0)),
        red
    );
    assert_eq!(
        colour(
            "@media (any-hover: none) { p { color: red } }",
            MediaEnv {
                hover: false,
                ..MediaEnv::default()
            }
        ),
        red
    );
    assert_eq!(
        colour("@media (pointer: fine) { p { color: red } }", env(1.0, 1.0)),
        red
    );
    assert_eq!(
        colour(
            "@media (any-pointer: coarse) { p { color: red } }",
            MediaEnv {
                pointer: Pointer::Coarse,
                ..MediaEnv::default()
            }
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (pointer: none) { p { color: red } }",
            MediaEnv {
                pointer: Pointer::None,
                ..MediaEnv::default()
            }
        ),
        red
    );
    assert_eq!(
        colour("@media (frobnicate: 3) { p { color: red } }", env(1.0, 1.0)),
        Color::BLACK,
        "an unknown feature is false"
    );
    assert_eq!(
        colour(
            "@media (min-width: 1em) { p { color: red } }",
            env(100.0, 1.0)
        ),
        red,
        "em in a query is 16px"
    );
    assert_eq!(
        colour(
            "@media (min-width: 10px) { @media (max-width: 300px) { p { color: red } } }",
            env(100.0, 1.0)
        ),
        red
    );
    assert_eq!(
        colour(
            "@media (min-width: 10px) { @media (max-width: 50px) { p { color: red } } }",
            env(100.0, 1.0)
        ),
        Color::BLACK
    );
    assert_eq!(
        colour(
            "p { @media (min-width: 10px) { color: red } }",
            env(100.0, 1.0)
        ),
        red,
        "inside a rule"
    );
    assert_eq!(
        colour(
            "@media (min-width: 10px), print { p { color: red } }",
            env(100.0, 1.0)
        ),
        red
    );
}

#[quench_test::test]
fn selectors_in_their_other_forms() {
    let n = sample();
    let m = |sel: &str, i: usize| selector(sel).matches(&n[i]);
    assert!(m("[aria-checked^=\"tr\"]", 4));
    assert!(m("[aria-checked$=\"ue\"]", 4));
    assert!(m("[aria-checked*=\"ru\"]", 4));
    assert!(m("[aria-checked|=\"true\"]", 4));
    assert!(m("[aria-checked~=\"true\"]", 4));
    assert!(m("[aria-checked=\"TRUE\" i]", 4));
    assert!(!m("[aria-checked=\"TRUE\"]", 4));
    assert!(
        !m("[aria-checked^=\"\"]", 4),
        "an empty prefix matches nothing"
    );
    assert!(m("button[aria-checked='true']", 4));
    let list = parse_selector_list(&tokenize("div ~ div, p"));
    assert!(
        list.iter().any(|s| s.matches(&n[3])),
        "a list matches any of its selectors"
    );
    assert!(!m("p ~ p", 3));
    assert!(m("p ~ button", 4));
    assert!(m(":not(.nope):not(#nope)", 3));
    assert!(m(":not(.a, .lead)", 4));
    assert!(!m(":not(.a, .lead)", 3));
    assert!(m("html > body > div#main > p", 3));
    assert!(m("html body div p", 3));
    assert!(!m("html > p", 3));
    assert!(m(":enabled", 4));
    assert!(!m(":disabled", 4));
    assert!(!m(":checked", 4));
    assert!(!m(":active", 4) && !m(":focus", 4) && !m(":focus-visible", 4));
    assert!(parse_selector_list(&tokenize("p,, q")).len() <= 2);
    assert_eq!(parse_selector_list(&tokenize("a, b, c")).len(), 3);
    let nested = quench_css::resolve_nesting(&selector(".a .b"), &selector("& .c"));
    assert_eq!(nested.unwrap().parts.len(), 3);
    assert!(
        quench_css::resolve_nesting(&selector(".a"), &selector(".b &")).is_none(),
        "`&` anywhere but first is not supported"
    );
    let combined = quench_css::resolve_nesting(&selector(".a"), &selector("&.b")).unwrap();
    assert_eq!(combined.parts.len(), 1);
    assert_eq!(combined.specificity(), 2 << 8);
    assert!(matches!(
        quench_css::Combinator::Child,
        quench_css::Combinator::Child
    ));
}

#[quench_test::test]
fn rules_and_at_rules_in_their_other_forms() {
    let sheet = parse(
        "@import url(x.css); @charset \"utf-8\"; @layer base { p { color: red } } @container (min-width: 1px) { p { margin: 1px } } @font-face { font-family: x } @page { margin: 0 } p { color: blue }",
    );
    assert!(sheet.ignored.iter().any(|i| i.contains("import")));
    assert!(sheet.ignored.iter().any(|i| i.contains("charset")));
    assert!(sheet.ignored.iter().any(|i| i.contains("font-face")));
    assert!(
        sheet
            .rules
            .iter()
            .filter(|r| r.declarations.iter().any(|d| d.name == "color"))
            .count()
            >= 2,
        "a layer's rules count"
    );
    let sheet = parse(
        "p { bogus-property: 1; color: red } :hover::before { color: blue } @supports (display: grid) and (color: red) { p { margin: 1px } } @supports (color: red) or (frobnicate: 1) { p { padding: 1px } } @supports not ((frobnicate: 1) and (color: red)) { p { opacity: 0.5 } }",
    );
    assert!(
        sheet.ignored.iter().any(|i| i.contains("bogus-property")),
        "{:?}",
        sheet.ignored
    );
    for property in ["margin", "padding", "opacity"] {
        assert!(
            sheet
                .rules
                .iter()
                .any(|r| r.declarations.iter().any(|d| d.name == property)),
            "{property}"
        );
    }
    let sheet = parse("a {b : c} /* unterminated");
    assert!(
        sheet.ignored.iter().any(|i| i.contains('b')),
        "b is not a property: {:?}",
        sheet.ignored
    );
    let sheet = parse("p { color: red; ; ; width: 1px }  q {}  ,  r { color: red }");
    assert!(sheet.rules.len() >= 2);
    let sheet = parse(
        "@media (min-width: 1px) { p { color: red } } @keyframes k { 0%, 50% { opacity: 0 } to { opacity: 1 } }",
    );
    assert_eq!(sheet.keyframes[0].frames.len(), 3);
    let kf = parse("@keyframes k { nonsense { opacity: 0 } }");
    assert!(kf.keyframes.is_empty() || kf.keyframes[0].frames.is_empty());
    assert!(parse("p { color: RED !IMPORTANT }").rules[0].declarations[0].important);
    let tokens = tokenize("a\\62 c 'str\\\ning' url(foo) 1e3 +.5 -x --y #1 @a ~= <!-- -->");
    assert!(tokens.len() > 8, "{tokens:?}");
    let values = parse_values(&tokenize("[a b] {c} (d (e)) f(g, h(i)) 1px/2px"));
    assert!(values.iter().any(|v| matches!(v, Value::Block('[', _))));
    assert!(values.iter().any(|v| matches!(v, Value::Block('{', _))));
    assert!(values.iter().any(|v| matches!(v, Value::Delim('/'))));
    assert_eq!(Value::Ident("x".into()).ident().as_deref(), Some("x"));
    assert_eq!(Value::Number(2.0).number(), Some(2.0));
    assert_eq!(Value::Percent(2.0).number(), None);
    assert!(
        quench_css::is_known("--anything")
            && quench_css::is_known("cursor")
            && !quench_css::is_known("grid-auto-flow")
    );
}

#[quench_test::test]
fn zoom_scales_absolute_lengths_but_not_relative_ones() {
    let nodes = sample();
    let mut cascade = Cascade::new(MediaEnv::default());
    cascade.zoom = 2.0;
    cascade.safe_area = [10.0, 0.0, 0.0, 0.0];
    cascade.add(&parse("p { width: 10px; height: 2em; padding-top: 1rem; margin-top: 1in; border: thin solid red; font-size: medium; top: env(safe-area-inset-top); left: 5vw }"));
    let s = cascade.compute(&nodes[3], None, 32.0);
    assert_eq!(
        s.width.as_ref().unwrap().as_px(),
        Some(20.0),
        "a CSS pixel is two device pixels"
    );
    assert_eq!(s.font_size, 32.0, "medium is 16 CSS pixels");
    assert_eq!(
        s.height.as_ref().unwrap().as_px(),
        Some(64.0),
        "em follows the (zoomed) font size"
    );
    assert_eq!(
        s.padding[0].as_px(),
        Some(32.0),
        "so does rem, from the root size the host gives"
    );
    assert_eq!(s.margin[0].as_ref().unwrap().as_px(), Some(192.0));
    assert_eq!(s.border_width[0], 2.0, "thin");
    assert_eq!(
        s.inset[0].as_ref().unwrap().as_px(),
        Some(10.0),
        "the safe area is already in device pixels"
    );
    assert_eq!(
        s.inset[3].as_ref().unwrap().as_px(),
        Some(128.0),
        "vw is of the device width: 5% of 2560"
    );
    let root = cascade.compute(&nodes[0], None, 32.0);
    assert_eq!(
        root.font_size, 32.0,
        "the root's default size is 16 CSS pixels"
    );
}

#[quench_test::test]
fn background_images_keep_their_address_size_repeat_and_place() {
    let s = style(
        "p { background: #123 url(data:image/png;base64,AAA=) no-repeat center }",
        3,
    );
    assert_eq!(
        s.background_image.as_deref(),
        Some("data:image/png;base64,AAA=")
    );
    assert!(!s.background_repeat);
    assert_eq!(s.background_position, (0.5, 0.5));
    let s = style(
        "p { background-image: url('a.png'); background-size: contain; background-position: right 25% } p { background-repeat: repeat }",
        3,
    );
    assert_eq!(s.background_image.as_deref(), Some("a.png"));
    assert_eq!(s.background_size, BackgroundSize::Contain);
    assert!(s.background_repeat);
    assert_eq!(s.background_position, (1.0, 0.25));
    assert_eq!(
        style("p { background-image: none }", 3).background_image,
        None
    );
    // A word the property does not take leaves the earlier value.
    let s = style("p { background-size: cover; background-size: banana }", 3);
    assert_eq!(s.background_size, BackgroundSize::Cover);
}

#[quench_test::test]
fn grid_properties_parse_tracks_repeats_and_lines() {
    use quench_css::Display;
    let s = style(
        "p { display: inline-grid; grid-template-columns: repeat(2, 1fr) [mid] 100px minmax(10px, 2fr) auto; \
         grid-template-rows: none; grid-auto-rows: 40px; gap: 4px 8px; grid-column: 2 / span 2; grid-row: -1 }",
        3,
    );
    assert_eq!(s.display, Display::InlineGrid);
    assert_eq!(
        s.grid_columns.tracks,
        [
            Track::Fr(1.0),
            Track::Fr(1.0),
            Track::Length(Length::Px(100.0)),
            Track::MinMax(
                Box::new(Track::Length(Length::Px(10.0))),
                Box::new(Track::Fr(2.0))
            ),
            Track::Auto
        ]
    );
    assert!(s.grid_rows.tracks.is_empty());
    assert_eq!(s.grid_auto_rows, Track::Length(Length::Px(40.0)));
    assert_eq!(
        (s.row_gap, s.column_gap),
        (Length::Px(4.0), Length::Px(8.0))
    );
    assert_eq!(s.grid_column, (GridLine::Line(2), GridLine::Span(2)));
    assert_eq!(s.grid_row, (GridLine::Line(-1), GridLine::Auto));
    let fill = style(
        "p { display: grid; grid-template-columns: repeat(auto-fill, minmax(100px, 1fr)) }",
        3,
    );
    assert_eq!(fill.display, Display::Grid);
    assert_eq!(
        fill.grid_columns
            .repeat_to_fill
            .as_ref()
            .map(|(t, fit)| (t.len(), *fit)),
        Some((1, false))
    );
    let fit = style("p { grid-template-columns: repeat(auto-fit, 50px) }", 3);
    assert_eq!(
        fit.grid_columns
            .repeat_to_fill
            .as_ref()
            .map(|(_, fit)| *fit),
        Some(true)
    );
    let ends = style(
        "p { grid-column-start: span 3; grid-column-end: auto; grid-row-start: 2; grid-row-end: 4; grid-gap: 6px }",
        3,
    );
    assert_eq!(
        (ends.grid_column, ends.grid_row),
        (
            (GridLine::Span(3), GridLine::Auto),
            (GridLine::Line(2), GridLine::Line(4))
        )
    );
    assert_eq!(ends.row_gap, Length::Px(6.0));
    // What is not a track or a line leaves the earlier value.
    let bad = style(
        "p { grid-template-columns: 1fr; grid-template-columns: banana; grid-column: 0; grid-row: 1 / 2 / 3 }",
        3,
    );
    assert_eq!(bad.grid_columns.tracks, [Track::Fr(1.0)]);
    assert_eq!(bad.grid_column, (GridLine::Auto, GridLine::Auto));
    assert_eq!(bad.grid_row, (GridLine::Auto, GridLine::Auto));
}

#[quench_test::test]
fn transitions_parse_as_a_shorthand_and_as_lists() {
    let s = style(
        "p { transition: opacity 0.3s ease-in-out 100ms, color 1s linear, background 200ms }",
        3,
    );
    assert_eq!(s.transitions.len(), 3);
    let first = &s.transitions[0];
    assert_eq!(
        (
            first.property.as_str(),
            first.duration,
            first.delay,
            first.timing
        ),
        ("opacity", 0.3, 0.1, Timing::EaseInOut)
    );
    assert_eq!(
        (s.transitions[1].duration, s.transitions[1].timing),
        (1.0, Timing::Linear)
    );
    assert_eq!(
        (
            s.transitions[2].property.as_str(),
            s.transitions[2].duration
        ),
        ("background", 0.2)
    );
    // The longhands make the list, cycling the shorter ones.
    let l = style(
        "p { transition-property: opacity, color, border-color; transition-duration: 1s, 2s; transition-timing-function: linear; transition-delay: 0.5s }",
        3,
    );
    let got: Vec<_> = l
        .transitions
        .iter()
        .map(|t| (t.property.as_str(), t.duration, t.delay, t.timing))
        .collect();
    assert_eq!(
        got,
        [
            ("opacity", 1.0, 0.5, Timing::Linear),
            ("color", 2.0, 0.5, Timing::Linear),
            ("border-color", 1.0, 0.5, Timing::Linear)
        ]
    );
    // Only a duration: every property, ease.
    let d = style("p { transition-duration: 150ms }", 3);
    assert_eq!(
        (
            d.transitions[0].property.as_str(),
            d.transitions[0].duration,
            d.transitions[0].timing
        ),
        ("all", 0.15, Timing::Ease)
    );
    assert!(style("p { transition: none }", 3).transitions.is_empty());
    assert_eq!(
        style(
            "p { transition: all 1s cubic-bezier(0.1, 0.7, 1, 0.1), x steps(4) 2s }",
            3
        )
        .transitions[1]
            .timing,
        Timing::Linear
    );
    // A value that is not a time leaves the earlier one.
    let bad = style("p { transition: color 1s; transition: color banana(3) }", 3);
    assert_eq!(bad.transitions.len(), 1);
}

#[quench_test::test]
fn grid_areas_parse_and_must_be_rectangular() {
    let s = style(
        "p { grid-template-areas: \"a a .\" \"b c c\"; grid-area: a }",
        3,
    );
    assert_eq!(s.grid_areas, [vec!["a", "a", "."], vec!["b", "c", "c"]]);
    assert_eq!(s.grid_area.as_deref(), Some("a"));
    let numbers = style("p { grid-area: 1 / 2 / 3 / 4 }", 3);
    assert_eq!(
        (numbers.grid_row, numbers.grid_column),
        (
            (GridLine::Line(1), GridLine::Line(3)),
            (GridLine::Line(2), GridLine::Line(4))
        )
    );
    assert_eq!(numbers.grid_area, None);
    let ragged = style("p { grid-template-areas: \"a b\" \"c\" }", 3);
    assert!(
        ragged.grid_areas.is_empty(),
        "rows of different lengths are refused"
    );
    assert!(
        style("p { grid-template-areas: none }", 3)
            .grid_areas
            .is_empty()
    );
    assert_eq!(style("p { grid-area: auto }", 3).grid_area, None);
    assert_eq!(
        style("p { grid-area: 1 / 2 / 3 / 4 / 5 }", 3).grid_row,
        (GridLine::Auto, GridLine::Auto)
    );
}

#[quench_test::test]
fn animations_parse_as_a_shorthand_and_as_lists_and_keyframes_are_kept() {
    let s = style(
        "p { animation: spin 2s linear 0.5s infinite alternate both, fade 300ms ease-in 3 }",
        3,
    );
    let first = &s.animations[0];
    assert_eq!(
        (
            first.name.as_str(),
            first.duration,
            first.delay,
            first.timing,
            first.iterations,
            first.direction,
            first.fill
        ),
        (
            "spin",
            2.0,
            0.5,
            Timing::Linear,
            f32::INFINITY,
            Direction::Alternate,
            FillMode::Both
        )
    );
    let second = &s.animations[1];
    assert_eq!(
        (
            second.name.as_str(),
            second.duration,
            second.iterations,
            second.timing
        ),
        ("fade", 0.3, 3.0, Timing::EaseIn)
    );
    let l = style(
        "p { animation-name: a, b; animation-duration: 1s; animation-delay: 1s, 2s; animation-iteration-count: 2; animation-direction: reverse; animation-fill-mode: forwards; animation-timing-function: ease-out }",
        3,
    );
    let got: Vec<_> = l
        .animations
        .iter()
        .map(|a| {
            (
                a.name.as_str(),
                a.duration,
                a.delay,
                a.iterations,
                a.direction,
                a.fill,
                a.timing,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                "a",
                1.0,
                1.0,
                2.0,
                Direction::Reverse,
                FillMode::Forwards,
                Timing::EaseOut
            ),
            (
                "b",
                1.0,
                2.0,
                2.0,
                Direction::Reverse,
                FillMode::Forwards,
                Timing::EaseOut
            )
        ]
    );
    assert!(style("p { animation: none }", 3).animations.is_empty());
    assert!(
        style("p { animation: x 1s; animation-name: none }", 3).animations[0]
            .name
            .is_empty()
    );
    let paused = style(
        "p { animation: x 1s paused running alternate-reverse backwards }",
        3,
    );
    assert_eq!(
        (paused.animations[0].direction, paused.animations[0].fill),
        (Direction::AlternateReverse, FillMode::Backwards)
    );
    assert_eq!(
        style("p { animation: x 1s; animation: 5 banana(1) }", 3)
            .animations
            .len(),
        1
    );
    // Keyframes reach the cascade and can be applied to a style.
    let mut cascade = Cascade::new(MediaEnv::default());
    cascade.add(&parse(
        "@keyframes k { from { opacity: 0 } to { opacity: 1; width: 9px } }",
    ));
    let k = cascade.keyframes("k").unwrap();
    assert_eq!(k.frames.len(), 2);
    let base = ComputedStyle::default();
    let made = cascade.style_with(&base, &base, 16.0, &k.frames[1].1);
    assert_eq!(
        (made.opacity, made.width.as_ref().and_then(Length::as_px)),
        (1.0, Some(9.0))
    );
    assert!(cascade.keyframes("nope").is_none());
}

#[quench_test::test]
fn touch_action_says_which_finger_movements_the_platform_may_take() {
    use quench_css::TouchAction;
    let of = |css: &str| style(&format!("div {{ {css} }}"), 2).touch_action;
    assert_eq!(of(""), TouchAction::AUTO);
    assert_eq!(of("touch-action: auto"), TouchAction::AUTO);
    assert_eq!(of("touch-action: manipulation"), TouchAction::AUTO);
    assert_eq!(of("touch-action: none"), TouchAction::NONE);
    let y = of("touch-action: pan-y");
    assert!(y.up && y.down && !y.left && !y.right);
    let up = of("touch-action: pan-x pan-up");
    assert!(up.left && up.right && up.up && !up.down);
    // Not inherited: a child of a `none` element is free again (the touch is judged by all of them together).
    let child = style("div { touch-action: none }", 3).touch_action;
    assert_eq!(child, TouchAction::AUTO);
    // What both allow.
    assert_eq!(
        y.and(up),
        TouchAction {
            left: false,
            right: false,
            up: true,
            down: false
        }
    );
    // The finger's movement decides by its larger axis.
    assert!(y.allows(2.0, -30.0) && y.allows(0.0, 30.0) && !y.allows(30.0, 2.0));
    assert!(up.allows(0.0, -5.0) && !up.allows(0.0, 5.0));
    // A value that is not one is ignored, as any invalid declaration is.
    assert_eq!(of("touch-action: sideways"), TouchAction::AUTO);
}

quench_test::main!();
