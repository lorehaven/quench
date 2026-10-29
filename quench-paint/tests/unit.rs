// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::use_self,
    clippy::field_reassign_with_default,
    clippy::cast_precision_loss,
    clippy::similar_names
)]

use std::rc::Rc;

use quench_css::{Cascade, ComputedStyle, Display, Element, MediaEnv, State, parse};
use quench_layout::{Fonts, Node, layout};
use quench_paint::paint;
use quench_raster::{Canvas, Color};

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
use T::{E, Text};

fn build(t: &T, parent: &ComputedStyle, next: &mut usize) -> Rc<Node> {
    *next += 1;
    let key = *next;
    match t {
        T::E(decls, children) => {
            let mut cascade = Cascade::new(MediaEnv::default());
            cascade.add(&parse(&format!("e {{ {decls} }}")));
            let mut style = cascade.compute(&One, Some(parent), 16.0);
            if !decls.contains("display") {
                style.display = Display::Block;
            }
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

/// Paints `tree` into a canvas of the given size (white page) and returns it.
fn render(tree: &T, w: usize, h: usize) -> Canvas {
    let fonts = Fonts::embedded();
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let node = build(tree, &root, &mut 0);
    let boxes = layout(&node, (w as f32, h as f32), &fonts);
    let mut canvas = Canvas::new(w, h);
    canvas.clear(Color::WHITE);
    paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|_| {
        Vec::new()
    });
    canvas
}

const RED: Color = Color::rgb(255, 0, 0);
const BLUE: Color = Color::rgb(0, 0, 255);
const WHITE: Color = Color::WHITE;

#[quench_test::test]
fn a_background_fills_the_border_box() {
    let c = render(
        &E(
            "",
            vec![E(
                "height: 20px; width: 30px; margin: 5px; padding: 3px; background: red",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(5, 5), RED);
    assert_eq!(c.pixel(5 + 36, 5 + 26), WHITE, "just past the padding box");
    assert_eq!(
        c.pixel(5 + 36 - 1, 5 + 26 - 1),
        RED,
        "the padding is painted"
    );
    assert_eq!(c.pixel(4, 4), WHITE);
}

#[quench_test::test]
fn rounded_corners_are_cut() {
    let c = render(
        &E(
            "",
            vec![E(
                "height: 40px; width: 40px; background: red; border-radius: 20px 0 0 0",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(0, 0), WHITE, "the top left corner is round");
    assert_eq!(c.pixel(39, 0), RED, "the others are square");
    assert_eq!(c.pixel(0, 39), RED);
    assert_eq!(c.pixel(20, 20), RED);
    let c = render(
        &E(
            "",
            vec![E(
                "height: 40px; width: 40px; background: red; border-radius: 50%",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(0, 0), WHITE);
    assert_eq!(c.pixel(20, 20), RED);
}

#[quench_test::test]
fn borders_are_drawn_inside_the_box_per_side() {
    let c = render(
        &E(
            "",
            vec![E(
                "height: 20px; width: 20px; border: 3px solid blue; background: red",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(0, 0), BLUE);
    assert_eq!(c.pixel(2, 10), BLUE);
    assert_eq!(c.pixel(3, 10), RED);
    assert_eq!(
        c.pixel(25, 25),
        BLUE,
        "the far corner: 20 + 2 * 3 = 26 wide"
    );
    let c = render(
        &E(
            "",
            vec![E(
                "height: 20px; width: 20px; border-bottom: 4px solid blue; border-left: 2px solid red",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(10, 23), BLUE);
    assert_eq!(c.pixel(10, 19), WHITE);
    assert_eq!(c.pixel(0, 5), RED);
    assert_eq!(c.pixel(2, 5), WHITE);
    let c = render(
        &E(
            "",
            vec![E(
                "height: 10px; width: 60px; border-top: 2px dashed blue",
                vec![],
            )],
        ),
        80,
        30,
    );
    let top: Vec<bool> = (0..60).map(|x| c.pixel(x, 0) == BLUE).collect();
    assert!(
        top.iter().any(|b| *b) && top.iter().any(|b| !*b),
        "dashes with gaps: {top:?}"
    );
}

#[quench_test::test]
fn shadows_fall_outside_and_inside() {
    let c = render(
        &E(
            "",
            vec![E(
                "margin: 30px; height: 20px; width: 20px; background: white; box-shadow: 0 6px 10px rgb(0 0 0 / 0.5)",
                vec![],
            )],
        ),
        80,
        80,
    );
    let below = c.pixel(40, 56);
    assert!(below.r < 250, "the shadow is below the box: {below:?}");
    assert_eq!(c.pixel(40, 40), WHITE, "the box covers its own shadow");
    assert_eq!(c.pixel(40, 2), WHITE, "far away nothing");
    let c = render(
        &E(
            "",
            vec![E(
                "margin: 10px; height: 40px; width: 40px; background: white; box-shadow: inset 0 0 0 4px blue",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(11, 30), BLUE);
    assert_eq!(c.pixel(30, 30), WHITE);
}

#[quench_test::test]
fn opacity_blends_the_whole_box() {
    let c = render(
        &E(
            "",
            vec![E(
                "height: 20px; width: 20px; background: red; opacity: 0.5",
                vec![E("height: 10px; width: 10px; background: blue", vec![])],
            )],
        ),
        40,
        40,
    );
    let outer = c.pixel(15, 15);
    assert!(
        outer.r > 250 && (120..=135).contains(&outer.g),
        "half red over white: {outer:?}"
    );
    let inner = c.pixel(5, 5);
    assert!(
        inner.r > 120 && inner.r < 135 && inner.b > 250,
        "the child is half there too, not red under blue: {inner:?}"
    );
}

#[quench_test::test]
fn overflow_hidden_clips_the_children_and_scroll_moves_them() {
    let tree = E(
        "",
        vec![E(
            "height: 20px; width: 20px; overflow: hidden",
            vec![E("height: 100px; width: 100px; background: red", vec![])],
        )],
    );
    let c = render(&tree, 60, 60);
    assert_eq!(c.pixel(10, 10), RED);
    assert_eq!(c.pixel(30, 10), WHITE);
    assert_eq!(c.pixel(10, 30), WHITE);
    let fonts = Fonts::embedded();
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let tree = E(
        "",
        vec![E(
            "height: 20px; width: 20px; overflow: auto",
            vec![
                E("height: 10px; width: 20px; background: red", vec![]),
                E("height: 10px; width: 20px; background: blue", vec![]),
                E("height: 40px", vec![]),
            ],
        )],
    );
    let node = build(&tree, &root, &mut 0);
    let boxes = layout(&node, (60.0, 60.0), &fonts);
    let mut canvas = Canvas::new(60, 60);
    canvas.clear(WHITE);
    paint(
        &mut canvas,
        &boxes,
        &fonts,
        &|key| if key == 2 { (0.0, 10.0) } else { (0.0, 0.0) },
        &|_| Vec::new(),
    );
    assert_eq!(canvas.pixel(5, 5), BLUE, "scrolled by one child");
}

#[quench_test::test]
fn text_is_painted_in_its_colour_on_the_line() {
    let c = render(
        &E(
            "",
            vec![E("color: blue; font-size: 24px", vec![Text("Hello")])],
        ),
        120,
        40,
    );
    let mut inked = 0;
    let mut stray = 0;
    for y in 0..40 {
        for x in 0..120 {
            let p = c.pixel(x, y);
            if p != WHITE {
                inked += 1;
                if p.r > p.b {
                    stray += 1;
                }
            }
        }
    }
    assert!(inked > 100, "{inked}");
    assert_eq!(stray, 0, "only shades of blue on white");
    assert_eq!(c.pixel(110, 20), WHITE, "and only where the text is");
}

#[quench_test::test]
fn an_inline_element_gets_its_own_background() {
    let c = render(
        &E(
            "",
            vec![E(
                "font-size: 20px",
                vec![
                    Text("ab "),
                    E(
                        "display: inline; background: red; padding: 0 6px",
                        vec![Text("cd")],
                    ),
                ],
            )],
        ),
        160,
        40,
    );
    let reds = (0..160)
        .filter(|x| (0..40).any(|y| c.pixel(*x, y) == RED))
        .count();
    assert!(reds > 20, "{reds}");
}

#[quench_test::test]
fn an_outline_goes_around_the_border_box_and_takes_no_room() {
    let c = render(
        &E(
            "",
            vec![E(
                "margin: 10px; height: 20px; width: 20px; outline: 3px solid blue; outline-offset: 2px",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(10, 10), WHITE, "the box has no background");
    assert_eq!(c.pixel(6, 20), BLUE, "2px away, 3px thick");
    assert_eq!(c.pixel(4, 20), WHITE);
    assert_eq!(c.pixel(8, 20), WHITE, "the offset leaves a gap");
}

#[quench_test::test]
fn positioned_boxes_paint_over_by_z_index_and_translate_moves_a_box() {
    let tree = E(
        "",
        vec![
            E(
                "position: absolute; z-index: 2; top: 0; left: 0; width: 20px; height: 20px; background: red",
                vec![],
            ),
            E(
                "position: absolute; z-index: 1; top: 0; left: 0; width: 30px; height: 30px; background: blue",
                vec![],
            ),
            E(
                "position: absolute; top: 40px; left: 0; width: 10px; height: 10px; background: red; transform: translateX(15px)",
                vec![],
            ),
        ],
    );
    let c = render(&tree, 60, 60);
    assert_eq!(c.pixel(5, 5), RED, "higher on top");
    assert_eq!(c.pixel(25, 25), BLUE);
    assert_eq!(c.pixel(5, 45), WHITE);
    assert_eq!(c.pixel(20, 45), RED, "moved by 15");
}

#[quench_test::test]
fn hidden_boxes_paint_nothing_but_keep_their_children_that_are_visible() {
    let c = render(
        &E(
            "",
            vec![
                E(
                    "visibility: hidden; height: 20px; width: 20px; background: red",
                    vec![E(
                        "visibility: visible; height: 10px; width: 10px; background: blue",
                        vec![],
                    )],
                ),
                E("display: none; height: 20px; background: red", vec![]),
            ],
        ),
        40,
        40,
    );
    assert_eq!(c.pixel(15, 15), WHITE);
    assert_eq!(c.pixel(5, 5), BLUE);
    assert_eq!(c.pixel(5, 25), WHITE);
}

#[quench_test::test]
fn decorations_are_drawn_on_the_box_they_are_for() {
    use quench_css::Color as Css;
    use quench_paint::Decoration;
    let fonts = Fonts::embedded();
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let tree = E(
        "",
        vec![E("width: 20px; height: 20px; margin: 5px", vec![])],
    );
    let node = build(&tree, &root, &mut 0);
    let boxes = layout(&node, (40.0, 40.0), &fonts);
    let draw = |d: Decoration| {
        let mut canvas = Canvas::new(40, 40);
        canvas.clear(WHITE);
        paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|key| {
            if key == 2 {
                vec![d.clone()]
            } else {
                Vec::new()
            }
        });
        canvas
    };
    let inked = |c: &Canvas| {
        (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .filter(|(x, y)| c.pixel(*x, *y) != WHITE)
            .count()
    };
    let check = draw(Decoration::Check(Css::rgba(0, 0, 255, 255)));
    assert!(
        inked(&check) > 30 && inked(&check) < 200,
        "{}",
        inked(&check)
    );
    assert_eq!(check.pixel(2, 2), WHITE, "outside the box nothing");
    let dot = draw(Decoration::Dot(Css::rgba(0, 0, 255, 255)));
    assert_eq!(dot.pixel(15, 15), BLUE, "the middle");
    assert_eq!(dot.pixel(6, 6), WHITE, "not the corners");
    let bar = draw(Decoration::Bar {
        x: 3.0,
        y: 2.0,
        width: 1.0,
        height: 10.0,
        color: Css::rgba(0, 0, 255, 255),
    });
    assert_eq!(inked(&bar), 10);
}

/// Lays out and paints with a scroll function; the root's key is 0 as in the native renderer.
fn render_scrolled(tree: &T, w: usize, h: usize, scroll: &dyn Fn(usize) -> (f32, f32)) -> Canvas {
    let fonts = Fonts::embedded();
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let mut next = 0;
    let node = build(tree, &root, &mut next);
    let boxes = layout(&node, (w as f32, h as f32), &fonts);
    let mut canvas = Canvas::new(w, h);
    canvas.clear(WHITE);
    paint(&mut canvas, &boxes, &fonts, scroll, &|_| Vec::new());
    canvas
}

#[quench_test::test]
fn a_scroll_container_shows_a_bar_that_follows_the_scroll() {
    let tree = E(
        "",
        vec![E(
            "width: 60px; height: 100px; overflow: auto",
            vec![E("height: 400px", vec![])],
        )],
    );
    let bar_y = |scrolled: f32| {
        let c = render_scrolled(&tree, 80, 120, &|k| {
            if k == 2 { (0.0, scrolled) } else { (0.0, 0.0) }
        });
        let x = 60 - 8;
        (0..100).find(|y| c.pixel(x, *y) != WHITE)
    };
    let top = bar_y(0.0).expect("a bar at the top");
    let bottom = bar_y(300.0).expect("a bar at the bottom");
    assert!(top < 5 && bottom > 50, "{top} {bottom}");
    // No bar when everything fits, or when the box does not scroll.
    let fits = E(
        "",
        vec![E(
            "width: 60px; height: 100px; overflow: auto",
            vec![E("height: 50px", vec![])],
        )],
    );
    let c = render_scrolled(&fits, 80, 120, &|_| (0.0, 0.0));
    assert!((0..100).all(|y| c.pixel(52, y) == WHITE));
    let hidden = E(
        "",
        vec![E(
            "width: 60px; height: 100px; overflow: hidden",
            vec![E("height: 400px", vec![])],
        )],
    );
    let c = render_scrolled(&hidden, 80, 120, &|_| (0.0, 0.0));
    assert!((0..100).all(|y| c.pixel(52, y) == WHITE));
}

#[quench_test::test]
fn the_document_scrolls_and_fixed_boxes_stay_put() {
    let fonts = Fonts::embedded();
    let mut base = ComputedStyle::default();
    base.display = Display::Block;
    // The root is key 0, as the native renderer numbers the document.
    let mut next = 100;
    let kids = [
        E("height: 100px; background: red", vec![]),
        E("height: 200px; background: green", vec![]),
        E(
            "position: fixed; top: 0; left: 0; width: 20px; height: 20px; background: blue",
            vec![],
        ),
    ]
    .iter()
    .map(|t| build(t, &base, &mut next))
    .collect();
    let node = Node::element(0, base, kids);
    let boxes = layout(&node, (60.0, 120.0), &fonts);
    let at = |scrolled: f32| {
        let mut canvas = Canvas::new(60, 120);
        canvas.clear(WHITE);
        paint(
            &mut canvas,
            &boxes,
            &fonts,
            &|k| if k == 0 { (0.0, scrolled) } else { (0.0, 0.0) },
            &|_| Vec::new(),
        );
        canvas
    };
    let top = at(0.0);
    assert_eq!(top.pixel(30, 50), RED);
    assert_eq!(top.pixel(30, 110), Color::rgb(0, 128, 0));
    assert_eq!(top.pixel(5, 5), BLUE);
    let down = at(100.0);
    assert_eq!(
        down.pixel(30, 50),
        Color::rgb(0, 128, 0),
        "the page moved up by 100"
    );
    assert_eq!(down.pixel(5, 5), BLUE, "the fixed box did not");
    let bar_top =
        (0..120).find(|y| top.pixel(55, *y) != Color::rgb(0, 128, 0) && top.pixel(55, *y) != RED);
    assert!(bar_top.is_some(), "the document has a scroll bar");
    let short = at(0.0);
    let _ = short;
}

#[quench_test::test]
fn borders_and_outlines_in_their_less_common_forms() {
    // Different widths and a radius: side by side, clipped to their strips.
    let c = render(
        &E(
            "",
            vec![E(
                "width: 30px; height: 30px; border-top: 4px solid red; border-bottom: 2px solid blue; border-radius: 10px",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(15, 1), RED);
    assert_eq!(c.pixel(15, 35), BLUE);
    // Dotted and dashed sides, vertical ones too.
    let c = render(
        &E(
            "",
            vec![E(
                "width: 40px; height: 40px; border-left: 2px dotted red; border-right: 2px dashed blue",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert!((0..40).any(|y| c.pixel(0, y) == RED) && (0..40).any(|y| c.pixel(0, y) == WHITE));
    assert!((0..40).any(|y| c.pixel(43, y) == BLUE));
    // A rounded outline follows the radius, and a hidden-border style paints nothing.
    let c = render(
        &E(
            "",
            vec![E(
                "margin: 10px; width: 20px; height: 20px; border-radius: 8px; outline: 2px solid red",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(20, 8), RED);
    assert_eq!(
        c.pixel(8, 8),
        WHITE,
        "the corner of the outline is round too"
    );
    let c = render(
        &E(
            "",
            vec![E("width: 20px; height: 20px; border: 4px none red", vec![])],
        ),
        60,
        60,
    );
    assert_eq!(c.pixel(0, 0), WHITE);
}

#[quench_test::test]
fn rounded_shadows_inset_shadows_and_clipped_rounded_boxes() {
    let c = render(
        &E(
            "",
            vec![E(
                "margin: 20px; width: 20px; height: 20px; border-radius: 10px; background: white; box-shadow: 0 0 6px 2px red, inset 0 0 4px 1px blue",
                vec![],
            )],
        ),
        60,
        60,
    );
    assert!(c.pixel(30, 30) != RED, "the inset shadow tints the inside");
    assert!(
        c.pixel(30, 17).r > 200 && c.pixel(30, 17).g < 200,
        "the outer one is red outside"
    );
    assert_eq!(c.pixel(1, 1), WHITE);
    // Fully transparent boxes paint nothing; a box with opacity and an overflowing child spills into its layer.
    let c = render(
        &E(
            "",
            vec![E(
                "width: 20px; height: 20px; background: red; opacity: 0",
                vec![],
            )],
        ),
        40,
        40,
    );
    assert_eq!(c.pixel(5, 5), WHITE);
    let c = render(
        &E(
            "",
            vec![E(
                "width: 10px; height: 10px; background: red; opacity: 0.5",
                vec![E(
                    "position: absolute; left: 15px; top: 0; width: 10px; height: 10px; background: blue",
                    vec![],
                )],
            )],
        ),
        40,
        40,
    );
    assert!(
        c.pixel(20, 5).b > 200 && c.pixel(20, 5).r > 100,
        "the child outside the box is part of the layer: {:?}",
        c.pixel(20, 5)
    );
}

#[quench_test::test]
fn a_background_image_is_sized_placed_tiled_and_kept_inside_the_padding_box() {
    use quench_css::BackgroundSize;
    use quench_paint::Decoration;
    let fonts = Fonts::embedded();
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    // A 20x20 box with a 2px border: the padding box is 16x16 at (7, 7) inside a 5px margin.
    let tree = E(
        "",
        vec![E(
            "width: 16px; height: 16px; margin: 5px; border: 2px solid rgb(0, 0, 255)",
            vec![],
        )],
    );
    let node = build(&tree, &root, &mut 0);
    let boxes = layout(&node, (40.0, 40.0), &fonts);
    // Two pixels side by side: red, then green.
    let rgba: Rc<Vec<u8>> = Rc::new(vec![255, 0, 0, 255, 0, 255, 0, 255]);
    let draw = |size: BackgroundSize, repeat: bool, position: (f32, f32), scale: f32| {
        let mut canvas = Canvas::new(40, 40);
        canvas.clear(WHITE);
        paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|key| {
            if key == 2 {
                vec![Decoration::Background {
                    rgba: Rc::clone(&rgba),
                    width: 2,
                    height: 1,
                    size,
                    repeat,
                    position,
                    scale,
                }]
            } else {
                Vec::new()
            }
        });
        canvas
    };
    let red = Color::rgb(255, 0, 0);
    let green = Color::rgb(0, 255, 0);
    // Natural size at scale 4: each tile is 8x4, so the padding box shows 2 tiles across and 4 down.
    let tiled = draw(BackgroundSize::Auto, true, (0.0, 0.0), 4.0);
    assert_eq!(tiled.pixel(8, 8), red);
    assert_eq!(tiled.pixel(7 + 7, 8), green);
    assert_eq!(
        tiled.pixel(7 + 8, 7 + 4),
        red,
        "the next tile, down and across"
    );
    assert_eq!(
        tiled.pixel(5, 5),
        Color::rgb(0, 0, 255),
        "the border is over it, not covered"
    );
    // Once, placed at the bottom right.
    let once = draw(BackgroundSize::Auto, false, (1.0, 1.0), 4.0);
    assert_eq!(once.pixel(8, 8), WHITE, "nothing at the top left");
    assert_eq!(
        once.pixel(7 + 15, 7 + 15),
        green,
        "the picture ends at the corner"
    );
    // Contain fits the width (2:1 in a square), cover fills it.
    let contain = draw(BackgroundSize::Contain, false, (0.0, 0.0), 1.0);
    assert_eq!(contain.pixel(9, 9), red);
    assert_eq!(
        contain.pixel(9, 7 + 12),
        WHITE,
        "below a picture that is 16x8"
    );
    let cover = draw(BackgroundSize::Cover, false, (0.0, 0.0), 1.0);
    assert_eq!(
        cover.pixel(9, 7 + 14),
        red,
        "16 high, 32 wide: the red half shows"
    );
    // An empty picture or box draws nothing.
    let none = draw(BackgroundSize::Auto, true, (0.0, 0.0), 0.0);
    assert_eq!(none.pixel(10, 10), WHITE);
}

#[quench_test::test]
fn italic_text_leans_to_the_right_without_changing_its_width() {
    let ink = |decls: &'static str| {
        let tree = E(decls, vec![Text("lllll")]);
        let c = render(&tree, 60, 30);
        // The ink's rows: where the leftmost non-white pixel is on the top row with ink and on the bottom row.
        let rows: Vec<Option<usize>> = (0..30)
            .map(|y| (0..60).find(|x| c.pixel(*x, y) != WHITE))
            .collect();
        let first = rows.iter().position(Option::is_some).unwrap();
        let last = rows.iter().rposition(Option::is_some).unwrap();
        (rows[first].unwrap(), rows[last].unwrap())
    };
    let (top_upright, bottom_upright) = ink("font-size: 20px; color: black");
    let (top_italic, bottom_italic) = ink("font-size: 20px; color: black; font-style: italic");
    assert!(
        top_upright.abs_diff(bottom_upright) <= 1,
        "upright letters are straight"
    );
    assert!(
        top_italic > bottom_italic + 2,
        "the top leans right of the bottom: {top_italic} {bottom_italic}"
    );
}

quench_test::main!();

#[quench_test::test]
fn boxes_far_outside_the_canvas_are_not_painted_but_the_ones_near_it_are() {
    // A column of 200 blocks of 100 px in a 120 px canvas: only the first few can be seen or reach in.
    let rows: Vec<T> = (0..200)
        .map(|_| E("height:100px;background:#f00", vec![]))
        .collect();
    let tree = E("", rows);
    let fonts = Fonts::embedded();
    let mut root = ComputedStyle::default();
    root.display = Display::Block;
    let node = build(&tree, &root, &mut 0);
    let boxes = layout(&node, (120.0, 120.0), &fonts);
    let mut canvas = Canvas::new(120, 120);
    canvas.clear(Color::WHITE);
    let asked = std::cell::RefCell::new(Vec::new());
    paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|key| {
        asked.borrow_mut().push(key);
        Vec::new()
    });
    // The first block is on screen, the second half on it.
    assert_eq!(canvas.pixel(10, 50), RED);
    assert_eq!(canvas.pixel(10, 110), RED);
    // Far fewer than the 200 blocks were visited: those within the margin of the canvas and no more.
    let visited = asked.borrow().len();
    assert!(visited < 12, "{visited} boxes were visited");
    assert!(visited >= 2, "the visible ones were: {visited}");
}
