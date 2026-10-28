// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::use_self,
    clippy::suboptimal_flops,
    clippy::field_reassign_with_default
)]

use std::rc::Rc;

use quench_css::{Cascade, ComputedStyle, Display, Element, MediaEnv, State, parse};
use quench_layout::{Fonts, Fragment, LayoutBox, Node, layout};

/// One element with no relatives: just enough to run the cascade over a rule written as `e { ... }`.
struct One;

impl Element for One {
    fn tag(&self) -> String {
        "e".into()
    }
    fn id(&self) -> Option<String> {
        None
    }
    fn has_class(&self, _: &str) -> bool {
        false
    }
    fn attribute(&self, _: &str) -> Option<String> {
        None
    }
    fn parent(&self) -> Option<Self> {
        None
    }
    fn previous_sibling(&self) -> Option<Self> {
        None
    }
    fn state(&self, _: State) -> bool {
        false
    }
}

enum T {
    E(&'static str, Vec<T>),
    Text(&'static str),
}

fn compute(decls: &str, parent: &ComputedStyle) -> ComputedStyle {
    let mut cascade = Cascade::new(MediaEnv::default());
    cascade.add(&parse(&format!("e {{ {decls} }}")));
    let mut style = cascade.compute(&One, Some(parent), 16.0);
    // `e` is not the root, so nothing makes it a block by itself.
    if !decls.contains("display") {
        style.display = Display::Block;
    }
    style
}

fn build(t: &T, parent: &ComputedStyle, next: &mut usize) -> Rc<Node> {
    *next += 1;
    let key = *next;
    match t {
        T::E(decls, children) => {
            let style = compute(decls, parent);
            let kids = children.iter().map(|c| build(c, &style, next)).collect();
            Node::element(key, style, kids)
        }
        T::Text(s) => {
            let mut inherited = parent.inherited();
            inherited.display = Display::Inline;
            Node::text(key, &Rc::new(inherited), s)
        }
    }
}

fn fonts() -> Fonts {
    Fonts::embedded()
}

/// Lays `tree` out in an 800 x 600 viewport; keys count up from 1 in document order.
fn run(tree: &T) -> LayoutBox {
    run_in(tree, (800.0, 600.0))
}

fn run_in(tree: &T, viewport: (f32, f32)) -> LayoutBox {
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let node = build(tree, &root, &mut 0);
    layout(&node, viewport, &fonts())
}

fn rect(b: &LayoutBox, key: usize) -> (f32, f32, f32, f32) {
    let (x, y) = b.position_of(key).unwrap();
    let found = b.find(key).unwrap();
    (x, y, found.width, found.height)
}

fn line_height() -> f32 {
    let style = ComputedStyle::default();
    fonts().line_height(&style)
}

use T::{E, Text};

fn lines(b: &LayoutBox) -> Vec<(String, f32, f32)> {
    b.fragments
        .iter()
        .filter_map(|f| match f {
            Fragment::Text(t) => Some((t.text.clone(), t.x, t.baseline)),
            Fragment::Inline(_) => None,
        })
        .collect()
}

#[quench_test::test]
fn blocks_stack_and_fill_their_container() {
    let b = run(&E(
        "",
        vec![
            E("height: 50px", vec![]),
            E("height: 30px; margin-top: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2), (0.0, 0.0, 800.0, 50.0));
    assert_eq!(rect(&b, 3), (0.0, 60.0, 800.0, 30.0));
    assert_eq!(rect(&b, 1).3, 90.0);
}

#[quench_test::test]
fn padding_and_borders_add_to_the_size_unless_border_box() {
    let b = run(&E(
        "",
        vec![
            E(
                "height: 20px; padding: 5px 10px; border: 2px solid red",
                vec![],
            ),
            E(
                "width: 100px; height: 20px; padding: 5px 10px; border: 2px solid red",
                vec![],
            ),
            E(
                "width: 100px; height: 20px; padding: 5px 10px; border: 2px solid red; box-sizing: border-box",
                vec![],
            ),
        ],
    ));
    assert_eq!((rect(&b, 2).2, rect(&b, 2).3), (800.0, 34.0));
    assert_eq!((rect(&b, 3).2, rect(&b, 3).3), (124.0, 34.0));
    assert_eq!((rect(&b, 4).2, rect(&b, 4).3), (100.0, 20.0));
    let content = b.find(3).unwrap().content_rect();
    assert_eq!(content, (12.0, 7.0, 100.0, 20.0));
}

#[quench_test::test]
fn percentages_min_and_max_sizes() {
    let b = run(&E(
        "",
        vec![
            E("width: 50%; height: 10px", vec![]),
            E("width: 90%; max-width: 300px; height: 10px", vec![]),
            E("width: 10px; min-width: 80px; height: 10px", vec![]),
            E("padding-left: 10%; height: 10px", vec![]),
            E("width: calc(100% - 20px); height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).2, 400.0);
    assert_eq!(rect(&b, 3).2, 300.0);
    assert_eq!(rect(&b, 4).2, 80.0);
    assert_eq!(
        b.find(5).unwrap().padding[3],
        80.0,
        "a percentage padding is of the width"
    );
    assert_eq!(rect(&b, 6).2, 780.0);
}

#[quench_test::test]
fn auto_margins_centre_a_block() {
    let b = run(&E(
        "",
        vec![
            E("width: 200px; height: 10px; margin: 0 auto", vec![]),
            E("width: 200px; height: 10px; margin-left: auto", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).0, 300.0);
    assert_eq!(rect(&b, 3).0, 600.0);
}

#[quench_test::test]
fn sibling_margins_collapse() {
    let b = run(&E(
        "",
        vec![
            E("height: 10px; margin-bottom: 20px", vec![]),
            E("height: 10px; margin: 30px 0 -5px", vec![]),
            E("height: 10px; margin-top: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 3).1, 10.0 + 30.0, "the larger of the two");
    assert_eq!(rect(&b, 4).1, 55.0, "negative and positive add");
}

#[quench_test::test]
fn a_childs_margin_passes_through_a_parent_without_padding_or_border() {
    let b = run(&E(
        "",
        vec![E("", vec![E("height: 10px; margin-top: 20px", vec![])])],
    ));
    assert_eq!(
        rect(&b, 2).1,
        20.0,
        "the parent moved, not the child inside it"
    );
    assert_eq!(rect(&b, 3).1, 20.0);
    assert_eq!(rect(&b, 2).3, 10.0);
    let b = run(&E(
        "",
        vec![E(
            "padding-top: 1px",
            vec![E("height: 10px; margin-top: 20px", vec![])],
        )],
    ));
    assert_eq!(rect(&b, 2).1, 0.0);
    assert_eq!(rect(&b, 3).1, 21.0);
    let b = run(&E(
        "",
        vec![E(
            "overflow: hidden",
            vec![E("height: 10px; margin-top: 20px", vec![])],
        )],
    ));
    assert_eq!(rect(&b, 2).1, 0.0, "overflow keeps the margin inside");
}

#[quench_test::test]
fn text_makes_lines_that_wrap_at_spaces() {
    let lh = line_height();
    let b = run(&E("", vec![E("", vec![Text("hello")])]));
    assert_eq!(rect(&b, 2).3, lh);
    let one = lines(b.find(2).unwrap());
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].0, "hello");
    let b = run(&E(
        "",
        vec![E("width: 70px", vec![Text("hello world again")])],
    ));
    let l = lines(b.find(2).unwrap());
    assert_eq!(
        l.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        ["hello", "world", "again"],
        "{l:?}"
    );
    assert_eq!(rect(&b, 2).3, 3.0 * lh);
    assert!(l[1].2 > l[0].2 && (l[1].2 - l[0].2 - lh).abs() < 0.01);
    // A word wider than the box overflows it; it is not cut.
    let b = run(&E("", vec![E("width: 10px", vec![Text("extraordinary")])]));
    assert_eq!(lines(b.find(2).unwrap()).len(), 1);
}

#[quench_test::test]
fn white_space_collapses_and_edges_are_trimmed() {
    let b = run(&E("", vec![E("", vec![Text("  a \n   b  ")])]));
    let l = lines(b.find(2).unwrap());
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].0, "a b");
    assert_eq!(l[0].1, 0.0, "no leading space");
}

#[quench_test::test]
fn text_align_moves_lines() {
    let at = |align: &'static str| {
        let b = run(&E(
            "",
            vec![E(
                Box::leak(format!("width: 200px; text-align: {align}").into_boxed_str()),
                vec![Text("hi")],
            )],
        ));
        let f = lines(b.find(2).unwrap());
        let width = match &b.find(2).unwrap().fragments[0] {
            Fragment::Text(t) => t.width,
            Fragment::Inline(_) => 0.0,
        };
        (f[0].1, width)
    };
    let (left, w) = at("left");
    assert_eq!(left, 0.0);
    assert_eq!(at("right").0, 200.0 - w);
    assert_eq!(at("center").0, (200.0 - w) / 2.0);
}

#[quench_test::test]
fn inline_blocks_sit_on_a_line_and_shrink_to_fit() {
    let b = run(&E(
        "",
        vec![
            E("display: inline-block; width: 100px; height: 30px", vec![]),
            E("display: inline-block; width: 50px; height: 20px", vec![]),
            E("display: inline-block", vec![Text("abc")]),
        ],
    ));
    let a = rect(&b, 2);
    let c = rect(&b, 3);
    let d = rect(&b, 4);
    assert_eq!((a.0, a.2), (0.0, 100.0));
    assert_eq!(c.0, 100.0, "no space between them: the markup has none");
    assert_eq!(d.0, 150.0);
    assert_eq!(
        d.2,
        fonts().measure(&ComputedStyle::default(), "abc"),
        "as wide as its text"
    );
    // They share a baseline: the bottom edges of the boxes without text.
    assert_eq!(a.1 + a.3, c.1 + c.3);
}

#[quench_test::test]
fn inline_blocks_wrap_when_the_line_is_full() {
    let b = run(&E(
        "width: 250px",
        vec![
            E("display: inline-block; width: 100px; height: 10px", vec![]),
            E("display: inline-block; width: 100px; height: 10px", vec![]),
            E("display: inline-block; width: 100px; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).0, 0.0);
    assert_eq!(rect(&b, 3).0, 100.0);
    assert_eq!(rect(&b, 4).0, 0.0);
    assert!(rect(&b, 4).1 > rect(&b, 3).1);
}

#[quench_test::test]
fn an_inline_element_reports_where_it_is_on_each_line() {
    let b = run(&E(
        "",
        vec![E(
            "width: 60px",
            vec![
                Text("one "),
                E(
                    "display: inline; padding: 0 4px; background: red",
                    vec![Text("two three")],
                ),
            ],
        )],
    ));
    let boxed = b.find(2).unwrap();
    let marks: Vec<_> = boxed
        .fragments
        .iter()
        .filter_map(|f| {
            if let Fragment::Inline(i) = f {
                Some((i.first, i.last, i.x, i.width))
            } else {
                None
            }
        })
        .collect();
    assert!(marks.len() >= 2, "{marks:?}");
    assert!(marks[0].0 && !marks[0].1);
    assert!(marks.last().unwrap().1 && !marks.last().unwrap().0);
}

#[quench_test::test]
fn a_flex_row_places_items_side_by_side() {
    let b = run(&E(
        "display: flex",
        vec![
            E("width: 100px; height: 20px", vec![]),
            E("width: 150px; height: 30px", vec![]),
            E("width: 50px; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2), (0.0, 0.0, 100.0, 20.0));
    assert_eq!(rect(&b, 3), (100.0, 0.0, 150.0, 30.0));
    assert_eq!(
        rect(&b, 4),
        (250.0, 0.0, 50.0, 10.0),
        "stretch does not touch an item with a height"
    );
    assert_eq!(rect(&b, 1).3, 30.0, "as tall as the tallest");
}

#[quench_test::test]
fn flex_grow_and_shrink_share_the_space() {
    let b = run(&E(
        "display: flex",
        vec![
            E("flex: 1; height: 10px", vec![]),
            E("flex: 3; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).2, 200.0);
    assert_eq!(rect(&b, 3).2, 600.0);
    assert_eq!(rect(&b, 3).0, 200.0);
    let b = run(&E(
        "display: flex",
        vec![
            E("width: 600px; height: 10px", vec![]),
            E("width: 400px; height: 10px", vec![]),
        ],
    ));
    assert!((rect(&b, 2).2 - 480.0).abs() < 0.01, "{:?}", rect(&b, 2));
    assert!((rect(&b, 3).2 - 320.0).abs() < 0.01);
    let b = run(&E(
        "display: flex",
        vec![
            E("width: 600px; height: 10px; flex-shrink: 0", vec![]),
            E("width: 400px; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).2, 600.0);
    assert_eq!(rect(&b, 3).2, 200.0);
    let b = run(&E(
        "display: flex",
        vec![
            E("flex: 1; max-width: 100px; height: 10px", vec![]),
            E("flex: 1; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).2, 100.0, "frozen at its maximum");
    assert_eq!(rect(&b, 3).2, 700.0, "the rest goes to the other");
}

#[quench_test::test]
fn flex_justify_and_gap() {
    let items = || {
        vec![
            E("width: 100px; height: 10px", vec![]),
            E("width: 100px; height: 10px", vec![]),
            E("width: 100px; height: 10px", vec![]),
        ]
    };
    let at = |decls: &'static str| -> Vec<f32> {
        let b = run(&E(decls, items()));
        (2..=4).map(|k| rect(&b, k).0).collect()
    };
    assert_eq!(
        at("display: flex; justify-content: flex-end"),
        [500.0, 600.0, 700.0]
    );
    assert_eq!(
        at("display: flex; justify-content: center"),
        [250.0, 350.0, 450.0]
    );
    assert_eq!(
        at("display: flex; justify-content: space-between"),
        [0.0, 350.0, 700.0]
    );
    let around = at("display: flex; justify-content: space-around");
    assert!(
        (around[0] - 500.0 / 6.0).abs() < 0.01
            && (around[1] - (500.0 / 6.0 * 3.0 + 100.0)).abs() < 0.01,
        "{around:?}"
    );
    let even = at("display: flex; justify-content: space-evenly");
    assert!(
        (even[0] - 125.0).abs() < 0.01 && (even[1] - 350.0).abs() < 0.01,
        "{even:?}"
    );
    assert_eq!(at("display: flex; gap: 10px"), [0.0, 110.0, 220.0]);
    assert_eq!(
        at("display: flex; flex-direction: row-reverse"),
        [700.0, 600.0, 500.0]
    );
}

#[quench_test::test]
fn flex_alignment_on_the_cross_axis() {
    let b = run(&E(
        "display: flex; height: 100px; align-items: center",
        vec![
            E("width: 10px; height: 20px", vec![]),
            E("width: 10px; height: 40px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).1, 40.0);
    assert_eq!(rect(&b, 3).1, 30.0);
    let b = run(&E(
        "display: flex; height: 100px",
        vec![
            E("width: 10px", vec![]),
            E("width: 10px; height: 40px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).3, 100.0, "stretched");
    assert_eq!(rect(&b, 3).3, 40.0);
    let b = run(&E(
        "display: flex; height: 100px; align-items: flex-end",
        vec![
            E("width: 10px; height: 20px", vec![]),
            E("width: 10px; height: 40px; align-self: flex-start", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).1, 80.0);
    assert_eq!(rect(&b, 3).1, 0.0);
}

#[quench_test::test]
fn flex_wrap_makes_lines() {
    let item = || E("width: 100px; height: 20px", vec![]);
    let b = run(&E(
        "display: flex; flex-wrap: wrap; width: 250px; gap: 5px",
        vec![item(), item(), item(), item(), item()],
    ));
    assert_eq!(rect(&b, 2).0, 0.0);
    assert_eq!(rect(&b, 3).0, 105.0);
    assert_eq!((rect(&b, 4).0, rect(&b, 4).1), (0.0, 25.0));
    assert_eq!(rect(&b, 6).1, 50.0);
    assert_eq!(rect(&b, 1).3, 70.0);
}

#[quench_test::test]
fn a_flex_column_stacks_and_grows_into_a_fixed_height() {
    let b = run(&E(
        "display: flex; flex-direction: column",
        vec![E("height: 30px", vec![]), E("height: 40px", vec![])],
    ));
    assert_eq!(rect(&b, 2), (0.0, 0.0, 800.0, 30.0));
    assert_eq!(rect(&b, 3), (0.0, 30.0, 800.0, 40.0));
    assert_eq!(rect(&b, 1).3, 70.0);
    let b = run(&E(
        "display: flex; flex-direction: column; height: 200px",
        vec![
            E("height: 30px", vec![]),
            E("flex: 1", vec![]),
            E("height: 20px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 3).3, 150.0);
    assert_eq!(rect(&b, 4).1, 180.0);
    let b = run(&E(
        "display: flex; flex-direction: column; align-items: center",
        vec![E("width: 100px; height: 10px", vec![])],
    ));
    assert_eq!(rect(&b, 2).0, 350.0);
}

#[quench_test::test]
fn auto_margins_in_a_flex_row_take_the_free_space() {
    let b = run(&E(
        "display: flex",
        vec![
            E("width: 100px; height: 10px", vec![]),
            E("width: 100px; height: 10px; margin-left: auto", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 3).0, 700.0);
    let b = run(&E(
        "display: flex; height: 100px",
        vec![E("width: 100px; height: 10px; margin: auto", vec![])],
    ));
    assert_eq!(rect(&b, 2).0, 350.0);
    assert_eq!(rect(&b, 2).1, 45.0);
}

#[quench_test::test]
fn flex_items_are_not_narrower_than_their_longest_word() {
    let b = run(&E(
        "display: flex; width: 100px",
        vec![
            E("", vec![Text("extraordinary")]),
            E("width: 80px; height: 10px", vec![]),
        ],
    ));
    let w = fonts().measure(&ComputedStyle::default(), "extraordinary");
    assert!(rect(&b, 2).2 >= w - 0.01, "{} < {w}", rect(&b, 2).2);
    let b = run(&E(
        "display: flex; width: 100px",
        vec![
            E("min-width: 0", vec![Text("extraordinary")]),
            E("width: 80px; height: 10px", vec![]),
        ],
    ));
    assert!(rect(&b, 2).2 < w, "an explicit min-width: 0 allows it");
}

#[quench_test::test]
fn text_in_a_flex_container_is_an_item() {
    let b = run(&E(
        "display: flex; justify-content: center",
        vec![Text("abc")],
    ));
    let w = fonts().measure(&ComputedStyle::default(), "abc");
    // The anonymous box around the text has no key of a node's: `usize::MAX`.
    let item = usize::MAX;
    assert!((lines(b.find(item).unwrap())[0].1 - 0.0).abs() < 0.01);
    assert!(
        (rect(&b, item).0 - (800.0 - w) / 2.0).abs() < 0.01,
        "{:?}",
        rect(&b, item)
    );
}

#[quench_test::test]
fn absolute_boxes_are_placed_against_the_nearest_positioned_ancestor() {
    let tree = E(
        "",
        vec![E(
            "position: relative; margin: 20px; width: 300px; height: 200px",
            vec![
                E(
                    "position: absolute; top: 10px; left: 30px; width: 50px; height: 40px",
                    vec![],
                ),
                E(
                    "position: absolute; right: 5px; bottom: 7px; width: 50px; height: 40px",
                    vec![],
                ),
                E("position: absolute; inset: 0; ", vec![]),
            ],
        )],
    );
    let b = run(&tree);
    assert_eq!(rect(&b, 2), (20.0, 20.0, 300.0, 200.0));
    assert_eq!(rect(&b, 3), (50.0, 30.0, 50.0, 40.0));
    assert_eq!(
        rect(&b, 4),
        (
            20.0 + 300.0 - 5.0 - 50.0,
            20.0 + 200.0 - 7.0 - 40.0,
            50.0,
            40.0
        )
    );
    assert_eq!(rect(&b, 5), (20.0, 20.0, 300.0, 200.0), "inset: 0 fills it");
}

#[quench_test::test]
fn absolute_boxes_do_not_take_part_in_the_flow() {
    let b = run(&E(
        "",
        vec![
            E("height: 10px", vec![]),
            E("position: absolute; width: 5px; height: 500px", vec![]),
            E("height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 4).1, 10.0);
    assert_eq!(rect(&b, 3).1, 10.0, "where it would have been");
    assert_eq!(rect(&b, 1).3, 20.0);
}

#[quench_test::test]
fn fixed_boxes_use_the_viewport() {
    let b = run_in(
        &E(
            "",
            vec![E(
                "position: relative; margin-top: 100px; height: 50px",
                vec![E(
                    "position: fixed; right: 16px; bottom: 16px; width: 56px; height: 56px",
                    vec![],
                )],
            )],
        ),
        (400.0, 300.0),
    );
    assert_eq!(
        rect(&b, 3),
        (400.0 - 16.0 - 56.0, 300.0 - 16.0 - 56.0, 56.0, 56.0)
    );
    let b = run_in(
        &E(
            "",
            vec![E(
                "position: fixed; inset: 0 0 0 auto; width: 100px",
                vec![],
            )],
        ),
        (400.0, 300.0),
    );
    assert_eq!(
        rect(&b, 2),
        (300.0, 0.0, 100.0, 300.0),
        "top and bottom give the height"
    );
}

#[quench_test::test]
fn relative_boxes_move_without_moving_their_neighbours() {
    let b = run(&E(
        "",
        vec![
            E(
                "position: relative; top: 5px; left: 7px; height: 10px",
                vec![],
            ),
            E("height: 10px", vec![]),
        ],
    ));
    assert_eq!((rect(&b, 2).0, rect(&b, 2).1), (7.0, 5.0));
    assert_eq!(rect(&b, 3).1, 10.0);
}

#[quench_test::test]
fn display_none_takes_no_room() {
    let b = run(&E(
        "",
        vec![
            E("height: 10px; display: none", vec![]),
            E("height: 20px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 3).1, 0.0);
    assert_eq!(rect(&b, 1).3, 20.0);
}

#[quench_test::test]
fn contents_that_overflow_are_measured_for_scrolling() {
    let b = run(&E(
        "",
        vec![E(
            "height: 100px; overflow: auto",
            vec![E("height: 500px; width: 900px", vec![])],
        )],
    ));
    let scroller = b.find(2).unwrap();
    assert_eq!(scroller.height, 100.0);
    assert_eq!(scroller.overflow, (900.0, 500.0));
    assert_eq!(
        b.overflow,
        (800.0, 100.0),
        "what a scroll container holds does not make its parent overflow"
    );
}

#[quench_test::test]
fn replaced_boxes_have_their_own_size() {
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let mut inline = ComputedStyle::default();
    inline.display = Display::InlineBlock;
    let node = Node::element(1, root, vec![Node::replaced(2, inline, 13.0, 13.0)]);
    let b = layout(&node, (800.0, 600.0), &fonts());
    assert_eq!(
        (b.find(2).unwrap().width, b.find(2).unwrap().height),
        (13.0, 13.0)
    );
    assert!(b.height >= 13.0);
}

#[quench_test::test]
fn a_line_break_ends_the_line() {
    let b = run(&E("", vec![E("", vec![Text("a\u{2028}b")])]));
    let l = lines(b.find(2).unwrap());
    assert_eq!(l.len(), 2);
    assert!(l[1].2 > l[0].2);
}

#[quench_test::test]
fn a_flex_row_is_as_tall_as_its_min_height_and_centres_its_items_in_it() {
    let b = run(&E(
        "display: flex; align-items: center; min-height: 56px",
        vec![E("width: 10px; height: 20px", vec![])],
    ));
    assert_eq!(rect(&b, 1).3, 56.0);
    assert_eq!(
        rect(&b, 2).1,
        18.0,
        "centred in the 56px, not in its own 20px"
    );
}

#[quench_test::test]
fn a_flex_column_centres_in_its_min_height() {
    let b = run(&E(
        "display: flex; flex-direction: column; justify-content: center; min-height: 100px",
        vec![E("height: 20px", vec![])],
    ));
    assert_eq!(rect(&b, 1).3, 100.0);
    assert_eq!(rect(&b, 2).1, 40.0);
    let b = run(&E(
        "display: flex; flex-direction: column; justify-content: center; min-height: 100px; max-height: 60px",
        vec![E("height: 20px", vec![])],
    ));
    assert_eq!(rect(&b, 2).1, 20.0, "the maximum wins over the minimum");
}

fn width_of(tree: &T, key: usize) -> f32 {
    rect(&run(tree), key).2
}

#[quench_test::test]
fn shrink_to_fit_follows_the_content_of_every_kind() {
    let word = |s: &str| fonts().measure(&ComputedStyle::default(), s);
    let inline_block = |inner: Vec<T>| E("display: inline-block", inner);
    // Text: as wide as the text.
    assert_eq!(
        width_of(&E("", vec![inline_block(vec![Text("hello world")])]), 2),
        word("hello world")
    );
    // Blocks: the widest child, with padding and margins counted.
    let blocks = inline_block(vec![
        E("width: 50px; height: 5px", vec![]),
        E(
            "width: 80px; height: 5px; padding: 0 10px; margin-right: 5px",
            vec![],
        ),
    ]);
    assert_eq!(width_of(&E("", vec![blocks]), 2), 105.0);
    // A flex row: the items side by side, and the gaps.
    let row = E(
        "display: inline-flex; gap: 4px",
        vec![
            E("width: 30px; height: 5px", vec![]),
            E("width: 40px; height: 5px", vec![]),
        ],
    );
    assert_eq!(width_of(&E("", vec![row]), 2), 74.0);
    // A flex column: the widest item.
    let column = E(
        "display: inline-flex; flex-direction: column",
        vec![
            E("width: 30px; height: 5px", vec![]),
            E("width: 40px; height: 5px", vec![]),
        ],
    );
    assert_eq!(width_of(&E("", vec![column]), 2), 40.0);
    // Wrapping flex: the narrowest the items can be is the widest of them.
    let tree = E(
        "width: 60px",
        vec![E(
            "display: inline-flex; flex-wrap: wrap",
            vec![
                E("width: 30px; height: 5px", vec![]),
                E("width: 40px; height: 5px", vec![]),
            ],
        )],
    );
    assert_eq!(width_of(&tree, 2), 60.0, "as wide as the room, and wrapped");
    // min/max widths and box-sizing of the inline block itself.
    let capped = E(
        "display: inline-block; max-width: 20px",
        vec![E("width: 50px; height: 5px", vec![])],
    );
    assert_eq!(width_of(&E("", vec![capped]), 2), 20.0);
    let floored = E(
        "display: inline-block; min-width: 70px",
        vec![E("width: 50px; height: 5px", vec![])],
    );
    assert_eq!(width_of(&E("", vec![floored]), 2), 70.0);
    let sized = E(
        "display: inline-block; width: 30px; padding: 5px; box-sizing: border-box",
        vec![],
    );
    assert_eq!(width_of(&E("", vec![sized]), 2), 30.0);
    // An inline block inside text counts as one unbreakable word.
    let nested = inline_block(vec![
        Text("ab "),
        inline_block(vec![E("width: 30px; height: 5px", vec![])]),
    ]);
    assert_eq!(
        width_of(&E("", vec![nested]), 2),
        word("ab") + word("x x") - word("xx") + 30.0
    );
    // Out of flow children add nothing.
    let with_abs = inline_block(vec![
        E("position: absolute; width: 500px; height: 5px", vec![]),
        E("width: 20px; height: 5px", vec![]),
    ]);
    assert_eq!(width_of(&E("", vec![with_abs]), 2), 20.0);
}

#[quench_test::test]
fn text_transform_changes_what_is_drawn_and_measured() {
    let drawn = |css: &'static str| {
        let b = run(&E("", vec![E(css, vec![Text("hello big world")])]));
        lines(b.find(2).unwrap()).first().unwrap().0.clone()
    };
    assert_eq!(drawn("text-transform: uppercase"), "HELLO BIG WORLD");
    assert_eq!(drawn("text-transform: lowercase"), "hello big world");
    assert_eq!(drawn("text-transform: capitalize"), "Hello Big World");
    assert_eq!(drawn("text-transform: none"), "hello big world");
}

#[quench_test::test]
fn fonts_give_metrics_and_line_heights() {
    let fonts = Fonts::embedded();
    let mut style = ComputedStyle::default();
    let regular = fonts.metrics(&style);
    assert_eq!(regular.normal, regular.ascent + regular.descent);
    assert_eq!(fonts.line_height(&style), regular.normal);
    style.line_height = quench_css::LineHeight::Number(2.0);
    assert_eq!(fonts.line_height(&style), 32.0);
    style.line_height = quench_css::LineHeight::Px(30.0);
    assert_eq!(fonts.line_height(&style), 30.0);
    style.font_weight = 700;
    assert!(
        fonts.measure(&style, "Hello") > fonts.measure(&ComputedStyle::default(), "Hello"),
        "bold is wider"
    );
    let width = |weight: u16| {
        let mut s = ComputedStyle::default();
        s.font_weight = weight;
        fonts.measure(&s, "Hello world")
    };
    assert!(
        width(300) < width(400) && width(400) < width(500) && width(500) < width(700),
        "light, regular, medium, bold"
    );
    assert_eq!(
        (width(100), width(350)),
        (width(300), width(300)),
        "thin is light"
    );
    assert_eq!((width(449), width(450)), (width(400), width(500)));
    assert_eq!(
        (width(599), width(600)),
        (width(500), width(700)),
        "semibold is bold"
    );
    assert!(
        fonts.space(&style) > 0.0 && fonts.space(&style) == fonts.space(&style),
        "cached"
    );
    let with_system = Fonts::system();
    assert!(with_system.measure(&ComputedStyle::default(), "abc") > 0.0);
    assert!(format!("{with_system:?}").contains("Fonts"));
}

#[quench_test::test]
fn flex_baseline_reverse_and_cross_margins() {
    // Baseline alignment: the texts share a baseline, whatever their sizes.
    let b = run(&E(
        "display: flex; align-items: baseline",
        vec![
            E("font-size: 32px", vec![Text("A")]),
            E("font-size: 12px", vec![Text("b")]),
        ],
    ));
    let baselines: Vec<f32> = b.children.iter().map(|c| c.y + c.baseline).collect();
    assert!((baselines[0] - baselines[1]).abs() < 0.5, "{baselines:?}");
    // Column reverse and row reverse with margins.
    let b = run(&E(
        "display: flex; flex-direction: column-reverse; height: 100px",
        vec![
            E("height: 10px", vec![]),
            E("height: 20px; margin-bottom: 5px", vec![]),
        ],
    ));
    assert!(
        rect(&b, 2).1 > rect(&b, 3).1,
        "the first item is last: {:?} {:?}",
        rect(&b, 2),
        rect(&b, 3)
    );
    // Cross-axis auto margins.
    let b = run(&E(
        "display: flex; height: 100px",
        vec![
            E("width: 10px; height: 10px; margin-top: auto", vec![]),
            E("width: 10px; height: 10px; margin-bottom: auto", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).1, 90.0);
    assert_eq!(rect(&b, 3).1, 0.0);
    // Wrapped lines share extra cross space.
    let b = run(&E(
        "display: flex; flex-wrap: wrap; width: 100px; height: 100px",
        vec![
            E("width: 60px; height: 10px", vec![]),
            E("width: 60px; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 3).1, 50.0, "two lines of 50px each");
    // A percentage basis, and an item that may not shrink below its content.
    let b = run(&E(
        "display: flex",
        vec![
            E("flex-basis: 25%; height: 10px", vec![]),
            E("flex: 1; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).2, 200.0);
    assert_eq!(rect(&b, 3).2, 600.0);
    // A clipped item has no automatic minimum.
    let b = run(&E(
        "display: flex; width: 50px",
        vec![E(
            "overflow: hidden; width: 100px; height: 10px",
            vec![Text("abcdefghijklmnop")],
        )],
    ));
    assert!(rect(&b, 2).2 <= 50.0 + 0.01);
    // An absolutely positioned child of a flex container, and a hidden one.
    let b = run(&E(
        "display: flex; position: relative",
        vec![
            E(
                "position: absolute; right: 0; top: 0; width: 5px; height: 5px",
                vec![],
            ),
            E("display: none", vec![]),
            E("width: 10px; height: 10px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).0, 795.0);
    assert_eq!(rect(&b, 4).0, 0.0);
    // Empty flex container.
    let b = run(&E("display: flex; padding: 3px", vec![]));
    assert_eq!(rect(&b, 1).3, 6.0);
}

#[quench_test::test]
fn heights_percentages_and_limits() {
    let b = run(&E(
        "height: 200px",
        vec![
            E("height: 50%", vec![]),
            E("height: 10px; max-height: 5px", vec![]),
            E("height: 10px; min-height: 30px", vec![]),
        ],
    ));
    assert_eq!(rect(&b, 2).3, 100.0);
    assert_eq!(rect(&b, 3).3, 5.0);
    assert_eq!(rect(&b, 4).3, 30.0);
    let b = run(&E(
        "",
        vec![E("height: 50%", vec![E("height: 7px", vec![])])],
    ));
    assert_eq!(
        rect(&b, 2).3,
        7.0,
        "a percentage of an unknown height is auto"
    );
    let b = run(&E(
        "",
        vec![E("display: inline-block; width: 50%; height: 10px", vec![])],
    ));
    assert_eq!(rect(&b, 2).2, 400.0);
    // A text node laid out on its own, and a replaced box with a style width.
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let text = Node::text(5, &Rc::new(ComputedStyle::default()), "abc");
    let b = layout(
        &Node::element(
            1,
            root.clone(),
            vec![Node::element(
                2,
                ComputedStyle {
                    display: Display::Block,
                    ..ComputedStyle::default()
                },
                vec![text],
            )],
        ),
        (100.0, 100.0),
        &fonts(),
    );
    assert_eq!(b.children.len(), 1);
    let mut img = ComputedStyle::default();
    img.display = Display::InlineBlock;
    img.width = Some(quench_css::Length::Px(40.0));
    let b = layout(
        &Node::element(1, root, vec![Node::replaced(2, img, 13.0, 13.0)]),
        (100.0, 100.0),
        &fonts(),
    );
    assert_eq!(b.find(2).unwrap().width, 40.0);
}

#[quench_test::test]
fn boxes_report_their_rectangles() {
    let b = run(&E(
        "",
        vec![E(
            "margin: 10px; padding: 5px; border: 2px solid red; width: 50px; height: 20px",
            vec![],
        )],
    ));
    let inner = b.find(2).unwrap();
    assert_eq!(inner.content_rect(), (7.0, 7.0, 50.0, 20.0));
    assert_eq!(inner.padding_rect(), (2.0, 2.0, 60.0, 30.0));
    assert!(b.find(99).is_none());
    assert!(b.position_of(99).is_none());
    assert_eq!(b.position_of(2), Some((10.0, 10.0)));
}

#[quench_test::test]
fn a_replaced_box_keeps_its_proportions_when_one_side_is_given() {
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let image = |decls: &str| {
        let mut style = compute(decls, &root);
        style.display = Display::InlineBlock;
        Node::replaced(2, style, 200.0, 100.0)
    };
    let size = |decls: &str| {
        let b = layout(
            &Node::element(1, root.clone(), vec![image(decls)]),
            (800.0, 600.0),
            &fonts(),
        );
        let found = b.find(2).unwrap();
        (found.width, found.height)
    };
    assert_eq!(size(""), (200.0, 100.0), "its own size");
    assert_eq!(size("width: 50px"), (50.0, 25.0));
    assert_eq!(size("height: 40px"), (80.0, 40.0));
    assert_eq!(
        size("width: 50px; height: 50px"),
        (50.0, 50.0),
        "both given: as asked"
    );
    assert_eq!(size("width: 50%"), (400.0, 200.0));
}

#[quench_test::test]
fn a_grid_shares_its_width_by_fr_and_fixed_tracks_and_flows_items_into_cells() {
    let b = run(&E(
        "display: grid; grid-template-columns: 100px 1fr 2fr; column-gap: 10px; row-gap: 5px; width: 400px",
        vec![
            E("height: 20px", vec![]),
            E("height: 30px", vec![]),
            E("height: 10px", vec![]),
            E("height: 40px", vec![]),
        ],
    ));
    // 400 - 100 - two gaps of 10 = 280 for 3fr: 1fr is 93.33.
    let fr = 280.0 / 3.0;
    assert_eq!(
        rect(&b, 2),
        (0.0, 0.0, 100.0, 20.0),
        "an item with a height keeps it; the row is as tall as the tallest"
    );
    let second = rect(&b, 3);
    assert!(
        (second.0 - 110.0).abs() < 0.01 && (second.2 - fr).abs() < 0.01,
        "{second:?}"
    );
    let third = rect(&b, 4);
    assert!(
        (third.0 - (110.0 + fr + 10.0)).abs() < 0.01 && (third.2 - 2.0 * fr).abs() < 0.01,
        "{third:?}"
    );
    // The fourth item starts the next row, after the row gap.
    assert_eq!(rect(&b, 5), (0.0, 35.0, 100.0, 40.0));
    assert_eq!(rect(&b, 1).3, 75.0, "30 + 5 + 40");
}

#[quench_test::test]
fn grid_items_can_be_placed_by_line_and_span_and_repeat_fills_the_box() {
    let b = run(&E(
        "display: grid; grid-template-columns: repeat(4, 50px); grid-auto-rows: 20px; width: 300px",
        vec![
            E("grid-column: 2 / 4", vec![]),
            E("grid-column: span 2", vec![]),
            E("grid-column: 1; grid-row: 3", vec![]),
            E("", vec![]),
        ],
    ));
    assert_eq!(
        rect(&b, 2),
        (50.0, 0.0, 100.0, 20.0),
        "from line 2 to line 4"
    );
    // Flowing starts at the first cell: two columns do not fit before the first item's, so a new row begins.
    assert_eq!(rect(&b, 3), (0.0, 20.0, 100.0, 20.0));
    assert_eq!(rect(&b, 4), (0.0, 40.0, 50.0, 20.0), "row 3, column 1");
    assert_eq!(
        (rect(&b, 5).0, rect(&b, 5).1),
        (100.0, 20.0),
        "the last takes the next free cell after the one before it"
    );
    let fill = run(&E(
        "display: grid; grid-template-columns: repeat(auto-fill, minmax(100px, 1fr)); width: 350px",
        vec![
            E("height: 10px", vec![]),
            E("height: 10px", vec![]),
            E("height: 10px", vec![]),
            E("height: 10px", vec![]),
        ],
    ));
    // 3 columns of 100px fit in 350: they share it, 116.67 each, and the fourth item starts a row.
    let first = rect(&fill, 2);
    assert!((first.2 - 350.0 / 3.0).abs() < 0.01, "{first:?}");
    assert_eq!(rect(&fill, 5).0, 0.0);
    assert_eq!(rect(&fill, 5).1, 10.0);
}

#[quench_test::test]
fn grid_tracks_follow_their_content_and_a_height_stretches_the_rows() {
    // Auto columns take their content and share what is left.
    let b = run(&E(
        "display: grid; grid-template-columns: auto auto; width: 300px",
        vec![
            E("width: 50px; height: 10px", vec![]),
            E("width: 100px; height: 10px", vec![]),
        ],
    ));
    // The tracks are 125 and 175: what they want, plus the 150 left over shared equally. The items keep their widths.
    let (a, c) = (rect(&b, 2), rect(&b, 3));
    assert_eq!((a.0, a.2, c.0, c.2), (0.0, 50.0, 125.0, 100.0));
    // Fixed rows, fr rows in a box with a height, and alignment inside a cell.
    let rows = run(&E(
        "display: grid; grid-template-rows: 30px 1fr; height: 200px; align-items: center",
        vec![E("height: 10px", vec![]), E("height: 20px", vec![])],
    ));
    assert_eq!(rect(&rows, 2).1, 10.0, "centred in 30");
    assert_eq!(rect(&rows, 3).1, 30.0 + 75.0, "centred in the rest, 170");
    // A margin counts inside the cell.
    let m = run(&E(
        "display: grid; grid-template-columns: 100px; width: 100px",
        vec![E("margin: 5px 10px; height: 20px", vec![])],
    ));
    assert_eq!(rect(&m, 2), (10.0, 5.0, 80.0, 20.0));
    assert_eq!(rect(&m, 1).3, 30.0);
    // An inline grid shrinks to its tracks.
    let inline = run(&E(
        "",
        vec![E(
            "display: inline-grid; grid-template-columns: 40px 60px",
            vec![E("height: 10px", vec![])],
        )],
    ));
    assert_eq!(rect(&inline, 2).2, 100.0);
}

#[quench_test::test]
fn named_grid_areas_place_items_and_the_template_sets_the_size_of_the_grid() {
    // A header across the top, a side column and the main part, with the tracks sized in the template.
    let b = run(&E(
        "display: grid; width: 300px; grid-template-columns: 100px 1fr; grid-template-rows: 20px 1fr 10px; height: 100px; \
         grid-template-areas: \"head head\" \"side main\" \"foot foot\"",
        vec![
            E("grid-area: main", vec![]),
            E("grid-area: head", vec![]),
            E("grid-area: side", vec![]),
            E("grid-area: foot", vec![]),
            E("grid-area: nowhere; height: 5px", vec![]),
        ],
    ));
    assert_eq!(
        rect(&b, 2),
        (100.0, 20.0, 200.0, 65.0),
        "main: the second column of the middle row"
    );
    assert_eq!(
        rect(&b, 3),
        (0.0, 0.0, 300.0, 20.0),
        "head spans both columns"
    );
    assert_eq!(rect(&b, 4), (0.0, 20.0, 100.0, 65.0));
    assert_eq!(rect(&b, 5), (0.0, 85.0, 300.0, 10.0));
    // The one that is in no area flows into the first free cell, after the areas' cells are taken: a new row.
    assert_eq!(
        rect(&b, 6).1,
        95.0,
        "in an implicit row that takes 5px from the fr row"
    );
    // Without column tracks the template's cells make the columns.
    let auto = run(&E(
        "display: grid; width: 200px; grid-template-areas: \"a b\"",
        vec![
            E("grid-area: b; height: 10px", vec![]),
            E("grid-area: a; height: 10px", vec![]),
        ],
    ));
    assert_eq!((rect(&auto, 3).0, rect(&auto, 2).0), (0.0, 100.0));
    // Numbers: row start, column start, row end, column end.
    let lines = run(&E(
        "display: grid; width: 90px; grid-template-columns: repeat(3, 30px); grid-auto-rows: 10px",
        vec![E("grid-area: 2 / 2 / 3 / 4", vec![])],
    ));
    assert_eq!(rect(&lines, 2), (30.0, 10.0, 60.0, 10.0));
}

quench_test::main!();
