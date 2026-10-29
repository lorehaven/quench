// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::rc::Rc;

use quench_layout::Fonts;
use quench_raster::{Canvas, Color};
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

fn page(
    css: &str,
    view: impl IntoView,
    w: usize,
    h: usize,
) -> (Rc<NativeRenderer>, Canvas, Rc<quench_layout::LayoutBox>) {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet(css);
    r.set_viewport(w as f32, h as f32);
    let mounted = mount(&r, &r.root(), view);
    let mut canvas = Canvas::new(w, h);
    let boxes = r.render(&mut canvas);
    std::mem::forget(mounted);
    (r, canvas, boxes)
}

const RED: Color = Color::rgb(255, 0, 0);
const WHITE: Color = Color::WHITE;

fn collect(b: &quench_layout::LayoutBox, out: &mut Vec<String>) {
    for f in &b.fragments {
        if let quench_layout::Fragment::Text(t) = f {
            out.push(t.text.clone());
        }
    }
    b.children.iter().for_each(|c| collect(c, out));
}

#[quench_test::test]
fn a_view_is_mounted_styled_laid_out_and_painted() {
    let view = div().class("box");
    let (r, canvas, boxes) = page(
        "body { margin: 0 } .box { width: 40px; height: 30px; background: red }",
        view,
        100,
        100,
    );
    assert_eq!(canvas.pixel(5, 5), RED);
    assert_eq!(canvas.pixel(50, 5), WHITE);
    let div = r.children(r.root())[0];
    assert_eq!(r.tag(div).as_deref(), Some("div"));
    assert_eq!(boxes.children[0].width, 100.0, "the body fills the window");
    assert_eq!(boxes.children[0].children[0].width, 40.0);
}

#[quench_test::test]
fn the_browser_style_gives_headings_paragraphs_and_the_body_margin() {
    let view = div()
        .child(quench_ui::prelude::h1().text("Title"))
        .child(p().text("text"));
    let (_, _, boxes) = page("", view, 400, 400);
    let body = &boxes.children[0];
    assert_eq!(body.margin[1], 8.0);
    assert_eq!(body.margin[3], 8.0);
    assert!(
        (body.margin[0] - 21.44).abs() < 0.01,
        "the heading's margin collapses with the body's: {}",
        body.margin[0]
    );
    assert!(
        (body.margin[2] - 16.0).abs() < 0.01,
        "and the paragraph's: {}",
        body.margin[2]
    );
    let h1 = &body.children[0].children[0];
    assert_eq!(h1.style.font_size, 32.0);
    assert_eq!(h1.style.font_weight, 700);
    assert!(
        (h1.margin[0] - 21.44).abs() < 0.01,
        "0.67em of 32px: {}",
        h1.margin[0]
    );
}

#[quench_test::test]
fn the_page_style_wins_over_the_browser_style_and_inline_style_over_both() {
    let view = div()
        .child(p().class("a").text("x"))
        .child(p().style("margin-top", "3px").text("y"));
    let (_, _, boxes) = page("p { margin: 5px 0 }", view, 400, 400);
    let outer = &boxes.children[0].children[0];
    assert_eq!(outer.children[0].margin[0], 5.0);
    assert_eq!(outer.children[1].margin[0], 3.0);
}

#[quench_test::test]
fn attributes_and_classes_reach_the_selectors() {
    let view = div()
        .child(
            quench_ui::prelude::button()
                .attr("aria-pressed", "true")
                .class("chip")
                .text("a"),
        )
        .child(quench_ui::prelude::button().class("chip").text("b"));
    let css = "body { margin: 0 } .chip { display: block; height: 10px; background: white; border: 0 } .chip[aria-pressed=\"true\"] { background: red }";
    let (_, canvas, _) = page(css, view, 60, 60);
    assert_eq!(canvas.pixel(5, 5), RED);
    assert_eq!(canvas.pixel(5, 15), WHITE);
}

#[quench_test::test]
fn nodes_removed_from_the_tree_disappear() {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet("body { margin: 0 } div { height: 10px; background: red }");
    r.set_viewport(20.0, 40.0);
    let a = quench_ui::Renderer::create_element(&*r, "div");
    let b = quench_ui::Renderer::create_element(&*r, "div");
    quench_ui::Renderer::insert(&*r, &r.root(), &a, None);
    quench_ui::Renderer::insert(&*r, &r.root(), &b, None);
    let mut canvas = Canvas::new(20, 40);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 15), RED);
    quench_ui::Renderer::remove(&*r, &r.root(), &a);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 15), WHITE, "the second div moved up");
    assert_eq!(canvas.pixel(5, 5), RED);
}

#[quench_test::test]
fn the_page_background_fills_the_window() {
    let (_, canvas, _) = page(
        "body { background: red; margin: 0 }",
        div().text("x"),
        30,
        30,
    );
    assert_eq!(
        canvas.pixel(29, 29),
        RED,
        "the body's background goes to the whole canvas"
    );
}

#[quench_test::test]
fn inputs_show_their_value_or_placeholder_and_a_checked_box_gets_a_mark() {
    let view = div()
        .child(
            input()
                .attr("type", "text")
                .attr("value", "abc")
                .attr("id", "a"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("placeholder", "name")
                .attr("id", "b"),
        )
        .child(
            input()
                .attr("type", "checkbox")
                .attr("checked", "")
                .attr("id", "c"),
        );
    let (r, canvas, boxes) = page("body { margin: 0 }", view, 300, 100);
    let fields = &boxes.children[0].children[0];
    let mut texts = Vec::new();
    collect(fields, &mut texts);
    assert_eq!(texts, ["abc", "name"], "{texts:?}");
    let checkbox = fields.children.last().unwrap();
    assert_eq!((checkbox.width, checkbox.height), (13.0, 13.0));
    let (cx, cy) = boxes.position_of(checkbox.key).unwrap();
    let mark = canvas.pixel(cx as usize + 1, cy as usize + 1);
    assert!(
        mark.r < 6 && (110..=125).contains(&mark.g) && mark.b > 250,
        "the checked colour: {mark:?}"
    );
    let _ = r;
}

#[quench_test::test]
fn media_and_safe_area_come_from_the_host() {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet("body { margin: 0; padding-top: env(safe-area-inset-top); background: white } @media (prefers-color-scheme: dark) { body { background: red } }");
    r.set_viewport(20.0, 20.0);
    r.set_safe_area([7.0, 0.0, 0.0, 0.0]);
    let _m = mount(&r, &r.root(), div().text("x"));
    let mut canvas = Canvas::new(20, 20);
    let boxes = r.render(&mut canvas);
    assert_eq!(boxes.children[0].padding[0], 7.0);
    assert_eq!(canvas.pixel(1, 1), RED, "dark is the default scheme");
    r.set_media(quench_css::MediaEnv {
        color_scheme: quench_css::ColorScheme::Light,
        ..quench_css::MediaEnv::default()
    });
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(1, 1), WHITE);
}

/// The text fragments under a box, in order.
fn texts(b: &quench_layout::LayoutBox) -> Vec<String> {
    let mut out = Vec::new();
    collect(b, &mut out);
    out
}

#[quench_test::test]
fn the_dom_can_be_walked_and_changed() {
    let r = NativeRenderer::new();
    let a = r.create_element("DIV");
    let t = r.create_text("hi");
    let anchor = r.create_anchor();
    r.insert(&r.root(), &a, None);
    r.insert(&a, &t, None);
    r.insert(&a, &anchor, Some(&t));
    r.set_attr(&a, "id", "x");
    r.set_attr(&a, "id", "y");
    r.set_attr(&a, "class", "k");
    assert_eq!(
        r.attrs(a),
        [
            ("id".to_string(), "y".to_string()),
            ("class".to_string(), "k".to_string())
        ]
    );
    r.remove_attr(&a, "class");
    assert_eq!(r.attr(a, "class"), None);
    assert_eq!(r.tag(a).as_deref(), Some("div"), "tags are lowercase");
    assert_eq!(r.tag(t), None);
    assert_eq!(r.kind(a), quench_ui_native::NodeKind::Element("div".into()));
    assert_eq!(r.kind(t), quench_ui_native::NodeKind::Text("hi".into()));
    assert_eq!(r.kind(anchor), quench_ui_native::NodeKind::Anchor);
    r.set_text(&t, "ho");
    assert_eq!(r.text_of(a), "ho");
    assert_eq!(r.children(a).len(), 2);
    assert_eq!(r.children(a)[0], anchor, "inserted before the text");
    r.set_style(&a, "color", "red");
    r.set_style(&a, "width", "3px");
    r.set_style(&a, "color", "");
    assert_eq!(r.styles(a), [("width".to_string(), "3px".to_string())]);
    assert!(r.styles(t).is_empty() && r.attrs(t).is_empty());
    r.insert(&r.root(), &t, None);
    assert_eq!(r.children(a).len(), 1, "an inserted node moves");
    r.remove(&a, &anchor);
    r.remove(&r.root(), &anchor);
    assert!(r.children(a).is_empty());
    assert_eq!(a.index(), 2);
    assert!(format!("{r:?}").contains("NativeRenderer"));
    r.listen(&a, quench_ui::EventKind::Click, std::rc::Rc::new(|_| {}));
    r.focus(&a);
    r.trap_focus(&a);
    let default = NativeRenderer::default();
    assert_eq!(default.children(default.root()).len(), 0);
}

#[quench_test::test]
fn form_controls_show_what_a_browser_shows() {
    let view = div()
        .child(input().attr("type", "password").attr("value", "abc"))
        .child(input().attr("type", "date").attr("value", "2024-03-09"))
        .child(input().attr("type", "date"))
        .child(input().attr("type", "time").attr("value", "15:07"))
        .child(input().attr("type", "time").attr("value", "00:30"))
        .child(input().attr("type", "time"))
        .child(input().attr("type", "radio").attr("checked", ""))
        .child(input().attr("type", "range"))
        .child(input().attr("type", "hidden"))
        .child(quench_ui::prelude::textarea().attr("placeholder", "say"))
        .child(
            quench_ui::prelude::textarea()
                .attr("rows", "3")
                .attr("value", "typed"),
        )
        .child(
            quench_ui::prelude::p()
                .text("a")
                .child(quench_ui::element("br"))
                .child(quench_ui::prelude::span().text("b")),
        );
    let (_, canvas, boxes) = page("body { margin: 0 }", view, 300, 400);
    let shown = texts(&boxes);
    for want in [
        "\u{2022}\u{2022}\u{2022}",
        "03/09/2024",
        "mm/dd/yyyy",
        "03:07 PM",
        "12:30 AM",
        "--:-- --",
        "say",
        "typed",
        "a",
        "b",
    ] {
        assert!(shown.iter().any(|t| t == want), "{want} in {shown:?}");
    }
    let radio = boxes.children[0].children[0]
        .children
        .iter()
        .find(|c| c.style.border_style[0] != quench_css::BorderStyle::None && c.width == 13.0)
        .unwrap();
    let (x, y) = boxes.position_of(radio.key).unwrap();
    assert_eq!(
        canvas.pixel(x as usize + 6, y as usize + 6),
        Color::rgb(255, 255, 255),
        "the dot of a selected radio button"
    );
    let textarea = boxes.children[0].children[0]
        .children
        .iter()
        .filter(|c| c.style.min_height.is_some() && c.height > 30.0)
        .count();
    assert!(textarea >= 1);
}

#[quench_test::test]
fn state_and_siblings_reach_the_selectors() {
    let view = div()
        .child(
            quench_ui::prelude::button()
                .attr("disabled", "")
                .class("one")
                .text("a"),
        )
        .child(quench_ui::prelude::button().class("two").text("b"))
        .child(
            input()
                .attr("type", "checkbox")
                .attr("checked", "")
                .class("three"),
        );
    let css = "body { margin: 0 } button, input { display: block; border: 0; padding: 0; height: 10px; background: white } .one:disabled { background: red } .one + .two { margin-left: 5px } .three:checked { margin-left: 9px } .two:enabled { color: red } .two:hover { color: blue }";
    let (_, canvas, boxes) = page(css, view, 60, 60);
    assert_eq!(canvas.pixel(58, 2), RED, "the disabled button");
    let row = &boxes.children[0].children[0];
    assert_eq!(row.children[1].margin[3], 5.0, "the sibling combinator");
    assert_eq!(row.children[2].margin[3], 9.0);
    assert_eq!(
        row.children[1].style.color,
        quench_css::Color::rgba(255, 0, 0, 255),
        ":enabled applies, :hover does not without a pointer"
    );
}

#[quench_test::test]
fn the_browser_sheet_styles_lists_links_and_inline_text() {
    let view = div()
        .child(
            quench_ui::prelude::ul()
                .child(quench_ui::prelude::li().text("one"))
                .child(quench_ui::prelude::li().text("two")),
        )
        .child(quench_ui::prelude::a().text("link"))
        .child(quench_ui::prelude::strong().text("bold"));
    let (_, _, boxes) = page("", view, 300, 200);
    let ul = &boxes.children[0].children[0].children[0];
    assert_eq!(ul.padding[3], 40.0);
    assert_eq!(ul.style.list_style, quench_css::ListStyle::Disc);
    assert_eq!(ul.children[0].style.display, quench_css::Display::ListItem);
    let all = texts(&boxes);
    for want in ["one", "two", "link", "bold"] {
        assert!(all.iter().any(|t| t == want), "{want} in {all:?}");
    }
}

use std::cell::{Cell, RefCell};

use quench_ui::KeyEvent;

fn key(k: &str) -> KeyEvent {
    KeyEvent::new(k)
}

fn ctrl(k: &str) -> KeyEvent {
    KeyEvent {
        ctrl: true,
        ..KeyEvent::new(k)
    }
}

/// A page with its layout ready for events (the DOM is drawn once, as a window would have).
fn live(
    css: &str,
    view: impl IntoView,
    w: usize,
    h: usize,
) -> (Rc<NativeRenderer>, quench_ui::Mount<NativeRenderer>) {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet(css);
    r.set_viewport(w as f32, h as f32);
    let mounted = mount(&r, &r.root(), view);
    let mut canvas = Canvas::new(w, h);
    r.render(&mut canvas);
    (r, mounted)
}

const FLAT: &str = "body { margin: 0 } button, input, textarea { display: block; box-sizing: border-box; width: 100px; height: 30px; margin: 0; padding: 0; border: 0; background: white }";

#[quench_test::test]
fn a_click_needs_the_pointer_to_go_down_and_up_on_the_same_thing() {
    let clicks = Rc::new(Cell::new(0));
    let (a, b) = (clicks.clone(), clicks.clone());
    let view = div()
        .child(button().text("one").on_click(move || a.set(a.get() + 1)))
        .child(button().text("two").on_click(move || b.set(b.get() + 10)));
    let (r, _m) = live(FLAT, view, 200, 100);
    r.pointer_down(10.0, 10.0);
    r.pointer_up(10.0, 10.0);
    assert_eq!(clicks.get(), 1);
    r.pointer_down(10.0, 10.0);
    r.pointer_up(10.0, 40.0);
    assert_eq!(clicks.get(), 1, "released on another button: not a click");
    r.pointer_down(10.0, 40.0);
    r.pointer_up(10.0, 40.0);
    assert_eq!(clicks.get(), 11);
    r.pointer_down(150.0, 90.0);
    r.pointer_up(150.0, 90.0);
    assert_eq!(clicks.get(), 11, "nothing there");
}

#[quench_test::test]
fn events_bubble_and_can_be_prevented() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (outer, inner) = (log.clone(), log.clone());
    let view = button()
        .on_click(move || outer.borrow_mut().push("button"))
        .child(quench_ui::prelude::span().text("label").on_click(move || {
            inner.borrow_mut().push("span");
        }));
    let (r, _m) = live(
        "body { margin: 0 } button { display: block; border: 0; padding: 0; width: 100px; height: 30px }",
        view,
        200,
        100,
    );
    let at = r.element_at(50.0, 15.0).unwrap();
    assert_eq!(
        r.tag(at).as_deref(),
        Some("span"),
        "the text belongs to its span, not to the button"
    );
    r.pointer_down(50.0, 15.0);
    r.pointer_up(50.0, 15.0);
    assert_eq!(
        *log.borrow(),
        ["span", "button"],
        "inner first, then up the tree"
    );
}

#[quench_test::test]
fn hover_and_active_change_what_is_drawn() {
    let view = button().text("");
    let css = "body { margin: 0 } button { display: block; border: 0; padding: 0; width: 20px; height: 20px; background: white } button:hover { background: red } button:active { background: blue }";
    let (r, _m) = live(css, view, 60, 40);
    let mut canvas = Canvas::new(60, 40);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 5), WHITE);
    r.take_dirty();
    r.pointer_move(5.0, 5.0);
    assert!(r.take_dirty(), "the pointer moved onto something: redraw");
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 5), RED);
    r.pointer_down(5.0, 5.0);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 5), Color::rgb(0, 0, 255));
    r.pointer_up(5.0, 5.0);
    r.pointer_move(50.0, 30.0);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 5), WHITE, "the pointer left");
}

#[quench_test::test]
fn checkboxes_toggle_by_click_by_label_and_by_space() {
    let view = div()
        .child(input().attr("type", "checkbox").attr("id", "c1"))
        .child(
            quench_ui::prelude::label()
                .child(input().attr("type", "checkbox").attr("id", "c2"))
                .child(quench_ui::prelude::span().text("agree to the terms")),
        )
        .child(
            input()
                .attr("type", "radio")
                .attr("name", "g")
                .attr("id", "r1")
                .attr("checked", ""),
        )
        .child(
            input()
                .attr("type", "radio")
                .attr("name", "g")
                .attr("id", "r2"),
        );
    let (r, _m) = live(
        "body { margin: 0 } input[type=checkbox], input[type=radio] { display: block; margin: 0; height: 20px; width: 20px } label { display: block; height: 20px }",
        view,
        300,
        200,
    );
    let boxes = r.layout();
    let checked = |id: &str| -> bool {
        let all = r.children(r.children(r.root())[0]);
        let find = |n: quench_ui_native::NodeId| -> Vec<quench_ui_native::NodeId> {
            std::iter::once(n).chain(r.children(n)).collect()
        };
        all.into_iter()
            .flat_map(find)
            .find(|n| r.attr(*n, "id").as_deref() == Some(id))
            .map(|n| r.attr(n, "checked").is_some())
            .unwrap()
    };
    assert!(!checked("c1"));
    r.pointer_down(5.0, 5.0);
    r.pointer_up(5.0, 5.0);
    assert!(checked("c1"));
    r.pointer_down(5.0, 5.0);
    r.pointer_up(5.0, 5.0);
    assert!(!checked("c1"), "again unchecks");
    // The label's text toggles its checkbox.
    let label_y = boxes.children[0].children[0].children[1].y + 5.0;
    r.pointer_down(80.0, label_y);
    r.pointer_up(80.0, label_y);
    assert!(checked("c2"), "clicking a label clicks its control");
    // Radio buttons are exclusive.
    let r2_y = boxes.children[0].children[0].children[3].y + 5.0;
    r.pointer_down(5.0, r2_y);
    r.pointer_up(5.0, r2_y);
    assert!(checked("r2") && !checked("r1"));
    // Space on the focused checkbox toggles it.
    r.pointer_down(5.0, 5.0);
    r.pointer_up(5.0, 5.0);
    assert!(checked("c1"));
    r.key_down(&key(" "));
    assert!(!checked("c1"));
}

#[quench_test::test]
fn tab_walks_the_focusable_elements_and_enter_presses_a_button() {
    let presses = Rc::new(Cell::new(0));
    let p = presses.clone();
    let view = div()
        .child(button().text("a"))
        .child(button().attr("disabled", "").text("skipped"))
        .child(input().attr("type", "text"))
        .child(button().text("c").on_click(move || p.set(p.get() + 1)))
        .child(quench_ui::prelude::a().text("no href"))
        .child(quench_ui::prelude::a().attr("href", "#x").text("link"));
    let css = format!(
        "{FLAT} a {{ display: block; height: 20px }} button:focus-visible, input:focus-visible, a:focus-visible {{ outline: 2px solid red }}"
    );
    let (r, _m) = live(&css, view, 300, 300);
    let order: Vec<String> = (0..5)
        .map(|_| {
            r.key_down(&key("Tab"));
            let f = r.focused().unwrap();
            format!(
                "{}{}",
                r.tag(f).unwrap(),
                r.attr(f, "href").unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(
        order,
        ["button", "input", "button", "a#x", "button"],
        "{order:?}"
    );
    r.key_down(&KeyEvent {
        shift: true,
        ..key("Tab")
    });
    assert_eq!(
        r.tag(r.focused().unwrap()).as_deref(),
        Some("a"),
        "shift+tab goes back"
    );
    r.key_down(&KeyEvent {
        shift: true,
        ..key("Tab")
    });
    r.key_down(&key("Enter"));
    assert_eq!(presses.get(), 1, "Enter on a focused button clicks it");
    r.key_down(&key(" "));
    assert_eq!(presses.get(), 2, "so does space");
    let mut canvas = Canvas::new(300, 300);
    r.render(&mut canvas);
    let boxes = r.layout();
    let (x, y) = boxes.position_of(r.focused().unwrap().index()).unwrap();
    assert_eq!(
        canvas.pixel(x as usize + 5, (y as usize).saturating_sub(1)),
        RED,
        "the keyboard focus ring"
    );
}

#[quench_test::test]
fn focus_and_blur_events_and_a_mouse_click_elsewhere() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (l1, l2, l3) = (log.clone(), log.clone(), log.clone());
    let view = div()
        .child(
            button()
                .text("a")
                .on(quench_ui::EventKind::Focus, move |_| {
                    l1.borrow_mut().push("focus a");
                })
                .on(quench_ui::EventKind::Blur, move |_| {
                    l2.borrow_mut().push("blur a");
                }),
        )
        .child(
            quench_ui::prelude::p()
                .text("plain text")
                .on_click(move || l3.borrow_mut().push("p")),
        );
    let (r, _m) = live(FLAT, view, 300, 200);
    r.pointer_down(5.0, 5.0);
    r.pointer_up(5.0, 5.0);
    assert!(r.focused().is_some());
    let p_at = 60.0;
    r.pointer_down(5.0, p_at);
    r.pointer_up(5.0, p_at);
    assert!(
        r.focused().is_none(),
        "a click on something that cannot take focus blurs"
    );
    assert_eq!(*log.borrow(), ["focus a", "blur a", "p"]);
    r.focus(&r.children(r.children(r.root())[0])[0]);
    assert!(r.focused().is_some(), "views can move focus");
}

#[quench_test::test]
fn typing_edits_the_value_and_tells_the_view() {
    let typed = Rc::new(RefCell::new(String::new()));
    let t = typed.clone();
    let view = input()
        .attr("type", "text")
        .attr("value", "ab")
        .on_input(move |v| *t.borrow_mut() = v.to_string());
    let (r, _m) = live(FLAT, view, 200, 60);
    r.pointer_down(90.0, 10.0);
    r.pointer_up(90.0, 10.0);
    let field = r.focused().unwrap();
    r.text("c");
    assert_eq!(*typed.borrow(), "abc");
    assert_eq!(r.attr(field, "value").as_deref(), Some("abc"));
    r.key_down(&key("Backspace"));
    assert_eq!(*typed.borrow(), "ab");
    r.key_down(&key("ArrowLeft"));
    r.text("X");
    assert_eq!(*typed.borrow(), "aXb");
    r.key_down(&key("Home"));
    r.key_down(&key("Delete"));
    assert_eq!(*typed.borrow(), "Xb");
    r.key_down(&key("End"));
    r.key_down(&KeyEvent {
        shift: true,
        ..key("ArrowLeft")
    });
    r.text("YZ");
    assert_eq!(*typed.borrow(), "XYZ", "typing replaces the selection");
    r.key_down(&ctrl("a"));
    r.key_down(&ctrl("c"));
    r.key_down(&key("ArrowRight"));
    r.key_down(&ctrl("v"));
    r.key_down(&ctrl("v"));
    assert_eq!(*typed.borrow(), "XYZXYZXYZ", "copy and paste");
    assert_eq!(
        r.take_clipboard_out().as_deref(),
        Some("XYZ"),
        "a copy is for the host to put on the system clipboard"
    );
    assert_eq!(r.take_clipboard_out(), None, "once");
    r.set_clipboard("sys");
    r.key_down(&ctrl("v"));
    assert_eq!(
        *typed.borrow(),
        "XYZXYZXYZsys",
        "a paste inserts what the host set"
    );
    r.key_down(&ctrl("a"));
    r.key_down(&ctrl("x"));
    assert_eq!(r.take_clipboard_out().as_deref(), Some("XYZXYZXYZsys"));
    assert_eq!(*typed.borrow(), "");
    r.text("a b c");
    r.key_down(&ctrl("Backspace"));
    assert_eq!(*typed.borrow(), "a b ", "control deletes a word");
    r.key_down(&ctrl("ArrowLeft"));
    r.key_down(&ctrl("ArrowRight"));
    r.text("line\nbreak\u{7}");
    assert_eq!(
        *typed.borrow(),
        "a blinebreak ",
        "a single line field takes no line breaks or controls"
    );
}

#[quench_test::test]
fn a_click_in_a_field_puts_the_caret_there_and_it_is_drawn() {
    let view = input().attr("type", "text").attr("value", "hello world");
    let css = "body { margin: 0 } input { display: block; box-sizing: border-box; width: 200px; height: 30px; padding: 0 4px; border: 0; background: white; color: black; font-size: 16px }";
    let (r, _m) = live(css, view, 220, 40);
    let boxes = r.layout();
    let field = r.children(r.root())[0];
    let frags = boxes.text_fragments(field.index());
    let (fx, _, frag) = &frags[0];
    // A click just right of the fifth character lands the caret after it.
    let five = Fonts::embedded().measure(&frag.style, "hello");
    r.pointer_down(fx + five + 1.0, 10.0);
    r.pointer_up(fx + five + 1.0, 10.0);
    r.text("!");
    assert_eq!(r.attr(field, "value").as_deref(), Some("hello! world"));
    let mut canvas = Canvas::new(220, 40);
    r.render(&mut canvas);
    let dark = (0..220)
        .filter(|x| (0..40).any(|y| canvas.pixel(*x, y).r < 60))
        .count();
    assert!(dark > 10, "text and a caret are drawn");
    r.key_down(&ctrl("a"));
    r.render(&mut canvas);
    let tinted =
        (0..220).any(|x| u16::from(canvas.pixel(x, 15).b) > u16::from(canvas.pixel(x, 15).r) + 40);
    assert!(tinted, "a selection is highlighted");
    r.set_caret_visible(false);
    assert!(r.take_dirty());
    r.key_down(&key("Enter"));
}

#[quench_test::test]
fn enter_in_a_field_submits_its_form_and_a_textarea_takes_the_line_break() {
    let submitted = Rc::new(Cell::new(0));
    let s = submitted.clone();
    let view = quench_ui::prelude::form()
        .on_submit(move || s.set(s.get() + 1))
        .child(input().attr("type", "text"))
        .child(quench_ui::prelude::textarea());
    let (r, _m) = live(FLAT, view, 200, 100);
    r.pointer_down(5.0, 10.0);
    r.pointer_up(5.0, 10.0);
    r.key_down(&key("Enter"));
    assert_eq!(submitted.get(), 1);
    r.pointer_down(5.0, 40.0);
    r.pointer_up(5.0, 40.0);
    r.text("a");
    r.key_down(&key("Enter"));
    r.text("b");
    let area = r.focused().unwrap();
    assert_eq!(r.attr(area, "value").as_deref(), Some("a\nb"));
    assert_eq!(submitted.get(), 1, "Enter in a textarea is not a submit");
}

#[quench_test::test]
fn the_wheel_scrolls_the_container_under_the_pointer_within_its_limits() {
    let scrolled = Rc::new(RefCell::new(Vec::new()));
    let s = scrolled.clone();
    let view = div()
        .attr("id", "scroller")
        .on(quench_ui::EventKind::Scroll, move |e| {
            if let quench_ui::Event::Scroll(v) = e {
                s.borrow_mut().push(v.top);
            }
        })
        .child(div().style("height", "300px").text("tall"));
    let css = "body { margin: 0 } #scroller { height: 100px; width: 100px; overflow: auto }";
    let (r, _m) = live(css, view, 200, 200);
    let key = r.children(r.root())[0].index();
    r.wheel(10.0, 10.0, 0.0, 60.0);
    assert_eq!(r.scroll_of(key), (0.0, 60.0));
    r.wheel(10.0, 10.0, 0.0, 500.0);
    assert_eq!(
        r.scroll_of(key),
        (0.0, 200.0),
        "stops at the end of the content"
    );
    r.wheel(10.0, 10.0, 0.0, 500.0);
    assert_eq!(
        *scrolled.borrow(),
        [60.0, 200.0],
        "no event when nothing moved"
    );
    r.wheel(10.0, 10.0, 0.0, -1000.0);
    assert_eq!(r.scroll_of(key).1, 0.0);
    r.wheel(150.0, 150.0, 0.0, 50.0);
    assert_eq!(r.scroll_of(key).1, 0.0, "the pointer was elsewhere");
    r.pointer_move(10.0, 10.0);
    r.key_down(&key_named("PageDown"));
    assert!(
        r.scroll_of(key).1 > 0.0,
        "keys scroll what is under the pointer"
    );
    r.key_down(&key_named("End"));
    assert_eq!(r.scroll_of(key).1, 200.0);
    r.key_down(&key_named("Home"));
    r.key_down(&key_named("ArrowDown"));
    assert_eq!(r.scroll_of(key).1, 40.0);
    r.key_down(&key_named("ArrowUp"));
    assert_eq!(r.scroll_of(key).1, 0.0);
    // What is at a point follows the scroll.
    r.wheel(10.0, 10.0, 0.0, 150.0);
    assert_eq!(
        r.tag(r.element_at(10.0, 10.0).unwrap()).as_deref(),
        Some("div")
    );
    let mut canvas = Canvas::new(200, 200);
    r.render(&mut canvas);
    let bar = (0..200).any(|x| canvas.pixel(x, 50).r < 230 && canvas.pixel(x, 50).r > 100);
    assert!(bar, "a scroll bar is drawn");
}

fn key_named(k: &str) -> KeyEvent {
    KeyEvent::new(k)
}

#[quench_test::test]
fn focus_stays_inside_a_trapped_node_and_listeners_can_stop_defaults() {
    let view = div().child(button().text("outside")).child(
        div()
            .attr("id", "dialog")
            .child(button().text("one"))
            .child(button().text("two")),
    );
    let (r, _m) = live(FLAT, view, 300, 200);
    let dialog = r.children(r.children(r.root())[0])[1];
    r.trap_focus(&dialog);
    let mut seen = Vec::new();
    for _ in 0..3 {
        r.key_down(&key("Tab"));
        seen.push(r.text_of(r.focused().unwrap()));
    }
    assert_eq!(seen, ["one", "two", "one"], "Tab cycles inside the dialog");
    let blocked = Rc::new(Cell::new(false));
    let b = blocked.clone();
    let view = input().attr("type", "text").on_keydown(move |k| {
        if k.key == "x" {
            quench_ui::event::prevent_default();
            b.set(true);
        }
    });
    let (r, _m) = live(FLAT, view, 200, 60);
    r.pointer_down(5.0, 5.0);
    r.pointer_up(5.0, 5.0);
    r.key_down(&key("x"));
    assert!(blocked.get());
    r.key_up(&key("x"));
}

#[quench_test::test]
fn an_input_method_composes_commits_and_deletes_around_the_caret() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (a, b, c) = (log.clone(), log.clone(), log.clone());
    let view = input()
        .attr("type", "text")
        .attr("value", "ab")
        .on(quench_ui::EventKind::CompositionStart, move |e| {
            if let quench_ui::Event::CompositionStart(v) = e {
                a.borrow_mut().push(format!("start {v}"));
            }
        })
        .on(quench_ui::EventKind::CompositionEnd, move |e| {
            if let quench_ui::Event::CompositionEnd(v) = e {
                b.borrow_mut().push(format!("end {v}"));
            }
        })
        .on_input(move |v| c.borrow_mut().push(format!("input {v}")));
    let css = "body { margin: 0 } input { display: block; box-sizing: border-box; width: 200px; height: 30px; padding: 0 4px; border: 0; background: white; color: black; font-size: 16px }";
    let (r, _m) = live(css, view, 220, 40);
    assert!(r.ime_state().is_none(), "no field has focus");
    assert!(!r.text_field_focused());
    r.pointer_down(190.0, 10.0);
    r.pointer_up(190.0, 10.0);
    assert!(r.text_field_focused());
    let state = r.ime_state().unwrap();
    assert_eq!(
        (state.text.as_str(), state.cursor, state.anchor),
        ("ab", 2, 2)
    );
    assert!(
        state.rect.0 > 4.0 && state.rect.3 > 10.0,
        "the caret is where the text ends: {:?}",
        state.rect
    );
    // Composition shows in the field, underlined, and is not part of the value yet.
    r.ime_apply(0, 0, None, Some(("ni", 2)));
    let field = r.children(r.root())[0];
    assert_eq!(r.attr(field, "value").as_deref(), Some("ab"));
    let mut canvas = Canvas::new(220, 40);
    let boxes = r.render(&mut canvas);
    let texts = texts(&boxes);
    assert!(texts.iter().any(|t| t == "abni"), "{texts:?}");
    assert!(
        r.ime_state().unwrap().rect.0 > state.rect.0,
        "the caret moved to the end of the composition"
    );
    let underlined = (0..220).any(|x| canvas.pixel(x, 23).r < 100 && x > 20);
    assert!(underlined, "the composition is underlined");
    // The commit replaces the composition.
    r.ime_apply(0, 0, Some("你"), Some(("", 0)));
    assert_eq!(r.attr(field, "value").as_deref(), Some("ab你"));
    assert_eq!(r.ime_state().unwrap().cursor, "ab你".len());
    // Delete the character before the caret (in bytes) and insert another in its place.
    r.ime_apply(3, 0, Some("c"), None);
    assert_eq!(r.attr(field, "value").as_deref(), Some("abc"));
    r.ime_apply(0, 1, None, None);
    assert_eq!(
        r.attr(field, "value").as_deref(),
        Some("abc"),
        "nothing after the caret to delete"
    );
    r.key_down(&key("Home"));
    r.ime_apply(0, 1, None, None);
    assert_eq!(
        r.attr(field, "value").as_deref(),
        Some("bc"),
        "delete after the caret"
    );
    assert_eq!(
        *log.borrow(),
        [
            "start ab",
            "input ab你",
            "end ab你",
            "input abc",
            "input bc"
        ]
    );
    // A composition with no cursor and a change outside a text field.
    r.ime_apply(0, 0, None, Some(("x", -1)));
    r.ime_apply(0, 0, None, None);
    r.pointer_down(215.0, 38.0);
    r.pointer_up(215.0, 38.0);
    r.ime_apply(0, 0, Some("lost"), None);
}

use quench_ui_native::{AccessAction, AccessNode, Role};

fn walk<'a>(n: &'a AccessNode, out: &mut Vec<&'a AccessNode>) {
    out.push(n);
    n.children.iter().for_each(|c| walk(c, out));
}

fn flat(n: &AccessNode) -> Vec<&AccessNode> {
    let mut out = Vec::new();
    walk(n, &mut out);
    out
}

#[quench_test::test]
fn the_accessibility_tree_has_roles_names_states_and_places() {
    let clicked = Rc::new(Cell::new(0));
    let c = clicked.clone();
    let view = div()
        .child(quench_ui::prelude::h1().text("Welcome"))
        .child(quench_ui::prelude::p().text("Some words."))
        .child(
            button()
                .attr("aria-label", "Close")
                .text("x")
                .on_click(move || c.set(c.get() + 1)),
        )
        .child(button().attr("disabled", "").text("Nope"))
        .child(
            quench_ui::prelude::label()
                .child(input().attr("type", "checkbox").attr("checked", ""))
                .child(quench_ui::prelude::span().text("Subscribe")),
        )
        .child(
            quench_ui::prelude::label()
                .attr("for", "mail")
                .text("Email"),
        )
        .child(
            input()
                .attr("type", "email")
                .attr("id", "mail")
                .attr("value", "a@b.c")
                .attr("required", "")
                .attr("aria-describedby", "hint"),
        )
        .child(
            quench_ui::prelude::p()
                .attr("id", "hint")
                .text("We never share it"),
        )
        .child(
            button()
                .attr("role", "switch")
                .attr("aria-checked", "true")
                .text("Alerts"),
        )
        .child(
            button()
                .attr("aria-expanded", "false")
                .attr("aria-pressed", "true")
                .text("More"),
        )
        .child(
            div()
                .attr("aria-hidden", "true")
                .child(button().text("Ghost")),
        )
        .child(div().style("display", "none").child(button().text("Gone")))
        .child(quench_ui::prelude::a().attr("href", "#x").text("Read more"))
        .child(
            quench_ui::prelude::ul()
                .child(quench_ui::prelude::li().text("one"))
                .child(quench_ui::prelude::li().text("two")),
        )
        .child(
            div().attr("role", "tablist").child(
                button()
                    .attr("role", "tab")
                    .attr("aria-selected", "true")
                    .text("First"),
            ),
        )
        .child(div().text("loose text"))
        .child(input().attr("type", "hidden").attr("value", "secret"));
    let (r, _m) = live(
        "body { margin: 0 } button, input { display: block; height: 20px; width: 100px }",
        view,
        300,
        600,
    );
    let tree = r.accessibility_tree();
    assert_eq!(tree.role, Role::Document);
    let all = flat(&tree);
    let by_name = |name: &str| {
        all.iter()
            .find(|n| n.name == name)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "no node named {name}: {:?}",
                    all.iter().map(|n| (&n.name, n.role)).collect::<Vec<_>>()
                )
            })
    };
    let heading = by_name("Welcome");
    assert_eq!((heading.role, heading.level), (Role::Heading, 1));
    let words = by_name("Some words.");
    assert_eq!(
        words.role,
        Role::Text,
        "a paragraph holds its words as text"
    );
    assert!(
        all.iter()
            .any(|n| n.role == Role::Paragraph && n.children.iter().any(|c| c.id == words.id))
    );
    let close = by_name("Close");
    assert_eq!(close.role, Role::Button, "aria-label wins over the content");
    assert!(
        close.actions.contains(&AccessAction::Click)
            && close.actions.contains(&AccessAction::Focus)
            && close.focusable
    );
    assert!(
        close.bounds.2 >= 100.0 && close.bounds.3 == 20.0,
        "{:?}",
        close.bounds
    );
    let nope = by_name("Nope");
    assert!(nope.disabled && !nope.focusable && !nope.actions.contains(&AccessAction::Click));
    let subscribe = all
        .iter()
        .find(|n| n.name == "Subscribe" && n.role == Role::Checkbox)
        .copied()
        .expect("the checkbox");
    assert_eq!(
        (subscribe.role, subscribe.checked),
        (Role::Checkbox, Some(true)),
        "named by the label around it"
    );
    let email = all
        .iter()
        .find(|n| n.name == "Email" && n.role == Role::TextBox)
        .copied()
        .expect("the field");
    assert_eq!(
        email.role,
        Role::TextBox,
        "named by the label that points at it"
    );
    assert_eq!(email.value.as_deref(), Some("a@b.c"));
    assert!(email.required);
    assert_eq!(email.description, "We never share it");
    assert_eq!(by_name("Alerts").role, Role::Switch);
    assert_eq!(by_name("Alerts").checked, Some(true));
    let more = by_name("More");
    assert_eq!((more.expanded, more.selected), (Some(false), Some(true)));
    assert!(
        !all.iter()
            .any(|n| n.name == "Ghost" || n.name == "Gone" || n.name == "secret"),
        "hidden things are not there"
    );
    assert_eq!(by_name("Read more").role, Role::Link);
    assert!(
        all.iter().any(|n| n.role == Role::List)
            && all.iter().filter(|n| n.role == Role::ListItem).count() == 2
    );
    assert_eq!(by_name("First").role, Role::Tab);
    assert!(all.iter().any(|n| n.role == Role::TabList));
    assert!(
        all.iter()
            .any(|n| n.role == Role::Text && n.name == "loose text"),
        "text of a plain div is a node"
    );
    assert!(tree.count() > 10);
    assert!(tree.find(close.id).is_some() && tree.find(9999).is_none());
    // Actions are carried out as the user would.
    assert!(r.perform(close.id, AccessAction::Click));
    assert_eq!(clicked.get(), 1);
    assert!(r.perform(email.id, AccessAction::Focus));
    assert_eq!(
        r.focused().map(quench_ui_native::NodeId::index),
        Some(email.id)
    );
    let tree = r.accessibility_tree();
    let focused = flat(&tree).into_iter().find(|n| n.focused).unwrap();
    assert_eq!(focused.id, email.id);
    assert!(focused.caret.is_some());
    assert!(r.perform(email.id, AccessAction::ScrollIntoView));
    assert!(!r.perform(99999, AccessAction::Click));
}

#[quench_test::test]
fn bounds_follow_scrolling_and_fixed_boxes_stay() {
    let view = div()
        .child(div().style("height", "400px").text("tall"))
        .child(button().text("below").attr("id", "below"))
        .child(
            button()
                .text("fixed")
                .style("position", "fixed")
                .style("top", "0")
                .style("left", "0"),
        );
    let (r, _m) = live(
        "body { margin: 0 } button { display: block; width: 50px; height: 20px }",
        view,
        100,
        100,
    );
    let at = |name: &str| {
        flat(&r.accessibility_tree())
            .into_iter()
            .find(|n| n.name == name)
            .map(|n| n.bounds)
            .unwrap()
    };
    let (below, fixed) = (at("below"), at("fixed"));
    assert!(below.1 >= 100.0, "off screen: {below:?}");
    r.wheel(10.0, 10.0, 0.0, 350.0);
    let after = at("below");
    assert!(after.1 < below.1 - 300.0, "scrolled into view: {after:?}");
    assert_eq!(at("fixed"), fixed);
    let id = flat(&r.accessibility_tree())
        .into_iter()
        .find(|n| n.name == "below")
        .unwrap()
        .id;
    r.wheel(10.0, 10.0, 0.0, -1000.0);
    assert!(r.perform(id, AccessAction::ScrollIntoView));
    assert!(
        at("below").1 + 20.0 <= 100.5,
        "scrolled so that it shows: {:?}",
        at("below")
    );
}

#[quench_test::test]
fn a_scale_makes_css_pixels_bigger_and_media_see_css_pixels() {
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet("body { margin: 0 } div { width: 100px; height: 20px; background: red } @media (max-width: 300px) { div { background: blue } }");
    r.set_viewport(600.0, 400.0);
    r.set_scale(2.0);
    let _m = mount(&r, &r.root(), div());
    let mut canvas = Canvas::new(600, 400);
    let boxes = r.render(&mut canvas);
    let b = &boxes.children[0].children[0];
    assert_eq!((b.width, b.height), (200.0, 40.0));
    assert_eq!(
        boxes.style.font_size, 32.0,
        "the text is 16 CSS pixels at 2x"
    );
    assert_eq!(boxes.children[0].margin[0], 0.0);
    assert_eq!(
        canvas.pixel(100, 20),
        Color::rgb(0, 0, 255),
        "600 device pixels at 2x are 300 CSS pixels: the narrow rule applies"
    );
    assert_eq!(canvas.pixel(250, 20), WHITE);
    r.set_scale(1.0);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(50, 10), RED, "at 1x the window is 600 wide");
    // A touch that turns into something else is not a click.
    let clicks = Rc::new(Cell::new(0));
    let c = clicks.clone();
    let (r, _m2) = live(
        FLAT,
        button().on_click(move || c.set(c.get() + 1)),
        200,
        100,
    );
    r.pointer_down(10.0, 10.0);
    r.pointer_cancel();
    r.pointer_up(10.0, 10.0);
    assert_eq!(clicks.get(), 0);
}

fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    quench_image::encode_png(width, height, &rgba.repeat((width * height) as usize))
}

#[quench_test::test]
fn images_come_from_data_urls_registered_bytes_and_a_loader() {
    let red = png(4, 2, [255, 0, 0, 255]);
    let data = format!(
        "data:image/png;base64,{}",
        quench_core::base64::STANDARD.encode(&red)
    );
    let view = div()
        .child(
            quench_ui::prelude::img()
                .attr("src", data.as_str())
                .attr("alt", "red")
                .attr("id", "a"),
        )
        .child(
            quench_ui::prelude::img()
                .attr("src", "registered.png")
                .attr("width", "8")
                .attr("id", "b"),
        )
        .child(
            quench_ui::prelude::img()
                .attr("src", "loaded.png")
                .attr("height", "6")
                .attr("id", "c"),
        )
        .child(
            quench_ui::prelude::img()
                .attr("src", "missing.png")
                .attr("alt", "a missing picture")
                .attr("id", "d"),
        );
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet("body { margin: 0 } div { display: block } img { display: block; margin: 0 }");
    r.set_viewport(100.0, 100.0);
    r.add_image("registered.png", &png(2, 2, [0, 255, 0, 255]))
        .unwrap();
    let blue = png(3, 3, [0, 0, 255, 255]);
    r.set_image_loader(move |src| (src == "loaded.png").then(|| blue.clone()));
    let _m = mount(&r, &r.root(), view);
    let mut canvas = Canvas::new(100, 100);
    let boxes = r.render(&mut canvas);
    let size = |id: &str| {
        let all = r.children(r.children(r.root())[0]);
        let n = all
            .into_iter()
            .find(|n| r.attr(*n, "id").as_deref() == Some(id))
            .unwrap();
        let b = boxes.find(n.index()).unwrap();
        (b.width, b.height, boxes.position_of(n.index()).unwrap())
    };
    assert_eq!((size("a").0, size("a").1), (4.0, 2.0), "its own size");
    assert_eq!(
        (size("b").0, size("b").1),
        (8.0, 8.0),
        "a width and the proportions"
    );
    assert_eq!(
        (size("c").0, size("c").1),
        (6.0, 6.0),
        "a height and the proportions"
    );
    let (ax, ay) = (size("a").2.0 as usize, size("a").2.1 as usize);
    assert_eq!(canvas.pixel(ax + 1, ay), Color::rgb(255, 0, 0));
    let (bx, by) = (size("b").2.0 as usize, size("b").2.1 as usize);
    assert_eq!(canvas.pixel(bx + 4, by + 4), Color::rgb(0, 255, 0));
    let (cx, cy) = (size("c").2.0 as usize, size("c").2.1 as usize);
    assert_eq!(canvas.pixel(cx + 2, cy + 2), Color::rgb(0, 0, 255));
    assert!(
        texts(&boxes).join(" ").contains("missing"),
        "what cannot be shown is described"
    );
    // At twice the density a picture is twice as big in device pixels.
    r.set_scale(2.0);
    let doubled = r.render(&mut canvas);
    let a = r.children(r.children(r.root())[0])[0];
    assert_eq!(doubled.find(a.index()).unwrap().width, 8.0);
    assert!(r.add_image("bad.png", b"not an image").is_err());
}

#[quench_test::test]
fn timers_channels_and_motion_preference_follow_the_hosts_ticks() {
    let r = Rc::new(NativeRenderer::new());
    let fired = Rc::new(Cell::new(0));
    let (a, b) = (fired.clone(), fired.clone());
    let _t = quench_ui::platform::after(20, move || a.set(a.get() + 1));
    let cancelled = quench_ui::platform::after(30, move || b.set(b.get() + 100));
    cancelled.cancel();
    assert!(
        r.next_wake()
            .is_some_and(|d| d <= std::time::Duration::from_millis(20))
    );
    assert!(!r.tick(), "nothing is due yet");
    std::thread::sleep(std::time::Duration::from_millis(40));
    assert!(r.tick());
    assert_eq!(
        fired.get(),
        1,
        "the timer ran once and the cancelled one never"
    );
    assert_eq!(r.next_wake(), None, "nothing left to wait for");
    assert!(quench_ui::platform::now() > 30.0);
    // Other threads hand data to the views.
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let tx = r.channel(move |n: u32| s.borrow_mut().push(n));
    assert!(r.next_wake().is_some(), "channels are looked at");
    std::thread::spawn(move || {
        tx.send(1).unwrap();
        tx.send(2).unwrap();
    })
    .join()
    .unwrap();
    assert!(
        seen.borrow().is_empty(),
        "handled on the views' thread, at a tick"
    );
    assert!(r.tick());
    assert_eq!(*seen.borrow(), [1, 2]);
    assert!(!r.tick());
    // The motion preference comes from the media the host sets.
    assert!(!quench_ui::platform::prefers_reduced_motion());
    r.set_media(quench_css::MediaEnv {
        reduced_motion: true,
        ..quench_css::MediaEnv::default()
    });
    assert!(quench_ui::platform::prefers_reduced_motion());
    // `@media` sees CSS pixels after the media are set at a scale.
    r.set_scale(2.0);
    r.set_viewport(600.0, 400.0);
    r.set_media(quench_css::MediaEnv::default());
    r.add_stylesheet("body { margin: 0; background: red } @media (max-width: 300px) { body { background: blue } }");
    let mut canvas = Canvas::new(600, 400);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(5, 5), Color::rgb(0, 0, 255));
}

#[quench_test::test]
fn the_page_owns_the_directions_touch_action_leaves_it() {
    use quench_css::TouchAction;
    let view = div()
        .class("outer")
        .child(button().class("row").text("row"));
    let (r, _m) = live(
        &format!("{FLAT} .outer {{ touch-action: pan-y }} .row {{ touch-action: pan-x pan-up }}"),
        view,
        200,
        100,
    );
    // The row and its surroundings together: what both allow is up.
    let t = r.touch_action_at(10.0, 10.0);
    assert_eq!(
        t,
        TouchAction {
            left: false,
            right: false,
            up: true,
            down: false
        }
    );
    assert!(t.allows(0.0, -20.0) && !t.allows(0.0, 20.0) && !t.allows(20.0, 0.0));
    // Nothing there: the platform's.
    assert_eq!(r.touch_action_at(150.0, 90.0), TouchAction::AUTO);
}

#[quench_test::test]
fn a_cancelled_touch_is_told_to_the_page_and_a_released_drag_is_no_click() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (c, d, e, f) = (log.clone(), log.clone(), log.clone(), log.clone());
    let view = button()
        .text("one")
        .on_click(move || c.borrow_mut().push("click"))
        .on(quench_ui::EventKind::PointerDown, move |_| {
            d.borrow_mut().push("down");
        })
        .on(quench_ui::EventKind::PointerUp, move |_| {
            e.borrow_mut().push("up");
        })
        .on(quench_ui::EventKind::PointerCancel, move |_| {
            f.borrow_mut().push("cancel");
        });
    let (r, _m) = live(FLAT, view, 200, 100);
    r.pointer_down(10.0, 10.0);
    r.pointer_cancel();
    r.pointer_up(10.0, 10.0);
    assert_eq!(
        *log.borrow(),
        ["down", "cancel", "up"],
        "a cancel reaches the page and nothing is clicked"
    );
    log.borrow_mut().clear();
    r.pointer_down(10.0, 10.0);
    r.pointer_release(10.0, 10.0);
    assert_eq!(
        *log.borrow(),
        ["down", "up"],
        "a drag that ends on what it began on is not a click"
    );
    log.borrow_mut().clear();
    // The finger that has wandered off still reaches the element it began on, and a drag is no click.
    r.pointer_down(10.0, 10.0);
    r.pointer_drag(150.0, 90.0);
    r.pointer_release(150.0, 90.0);
    assert_eq!(*log.borrow(), ["down", "up"]);
}

quench_test::main!();

#[quench_test::test]
fn a_background_image_is_tiled_sized_and_placed_in_the_padding_box() {
    let side = |top_left: [u8; 4], rest: [u8; 4]| {
        // A 2x2 picture with one different pixel.
        let mut px = rest.repeat(4);
        px[..4].copy_from_slice(&top_left);
        quench_image::encode_png(2, 2, &px)
    };
    let tile = side([255, 0, 0, 255], [0, 0, 255, 255]);
    let r = Rc::new(NativeRenderer::new());
    r.add_image("tile.png", &tile).unwrap();
    let boxes = |css: &str| {
        let r = Rc::new(NativeRenderer::new());
        r.add_image("tile.png", &tile).unwrap();
        r.add_stylesheet(&format!(
            "body {{ margin: 0 }} div {{ width: 8px; height: 8px; border: 2px solid rgb(0, 255, 0); {css} }}"
        ));
        r.set_viewport(40.0, 40.0);
        let _m = mount(&r, &r.root(), div());
        let mut canvas = Canvas::new(40, 40);
        r.render(&mut canvas);
        canvas
    };
    // Tiles of 2x2 start at the padding box (2, 2) and repeat.
    let tiled = boxes("background-image: url(tile.png)");
    assert_eq!(tiled.pixel(2, 2), Color::rgb(255, 0, 0));
    assert_eq!(tiled.pixel(3, 2), Color::rgb(0, 0, 255));
    assert_eq!(tiled.pixel(4, 2), Color::rgb(255, 0, 0), "the next tile");
    assert_eq!(
        tiled.pixel(0, 0),
        Color::rgb(0, 255, 0),
        "the border is on top"
    );
    // No repeat: one tile, the rest is the colour behind.
    let once = boxes("background: rgb(9, 9, 9) url(tile.png) no-repeat");
    assert_eq!(once.pixel(2, 2), Color::rgb(255, 0, 0));
    assert_eq!(once.pixel(4, 2), Color::rgb(9, 9, 9));
    // Cover: the 8x8 padding box shows the picture four times larger.
    let cover = boxes(
        "background-image: url(tile.png); background-size: cover; background-repeat: no-repeat",
    );
    assert_eq!(cover.pixel(2, 2), Color::rgb(255, 0, 0));
    // Scaled up and smoothed: the corner is still red, and red reaches further than one pixel.
    assert!(cover.pixel(3, 3).r > 150, "{:?}", cover.pixel(3, 3));
    assert!(cover.pixel(9, 9).b > 200, "{:?}", cover.pixel(9, 9));
    drop(r);
}

#[quench_test::test]
fn drawings_come_from_data_urls_and_are_made_again_at_the_density() {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="6"><rect width="10" height="6" fill="#f00"/></svg>"##;
    let escaped = svg
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('#', "%23")
        .replace('"', "'");
    let page = |src: String| {
        let r = Rc::new(NativeRenderer::new());
        r.add_stylesheet("body { margin: 0 } img { display: block }");
        r.set_viewport(60.0, 40.0);
        let m = mount(
            &r,
            &r.root(),
            div().child(
                quench_ui::prelude::img()
                    .attr("src", src.as_str())
                    .attr("alt", "d"),
            ),
        );
        (r, m)
    };
    let (r, _m) = page(format!("data:image/svg+xml,{escaped}"));
    let mut canvas = Canvas::new(60, 40);
    let boxes = r.render(&mut canvas);
    let img = r.children(r.children(r.root())[0])[0];
    let found = boxes.find(img.index()).unwrap();
    assert_eq!((found.width, found.height), (10.0, 6.0), "its own size");
    assert_eq!(canvas.pixel(5, 3), Color::rgb(255, 0, 0));
    // At twice the density the box is twice as big and the drawing is sharp, not a stretched small one.
    r.set_scale(2.0);
    let doubled = r.render(&mut canvas);
    assert_eq!(doubled.find(img.index()).unwrap().width, 20.0);
    assert_eq!(canvas.pixel(19, 11), Color::rgb(255, 0, 0));
    // The same through base64 and through registered bytes.
    let b64 = quench_core::base64::STANDARD.encode(svg);
    let (r, _m) = page(format!("data:image/svg+xml;base64,{b64}"));
    let boxes = r.render(&mut Canvas::new(60, 40));
    let img = r.children(r.children(r.root())[0])[0];
    assert_eq!(boxes.find(img.index()).unwrap().width, 10.0);
    assert!(r.add_image("a.svg", svg.as_bytes()).is_ok());
    assert!(r.add_image("b.svg", b"<svg").is_err());
}

#[quench_test::test]
fn sliders_and_progress_bars_have_a_range_and_assistive_technology_can_set_a_fields_text() {
    let typed = Rc::new(std::cell::RefCell::new(String::new()));
    let t = typed.clone();
    let view = div()
        .child(
            input()
                .attr("type", "range")
                .attr("min", "10")
                .attr("max", "20")
                .attr("value", "12")
                .attr("aria-label", "Level"),
        )
        .child(input().attr("type", "range").attr("aria-label", "Middle"))
        .child(
            quench_ui::view::element("progress")
                .attr("value", "0.25")
                .attr("aria-label", "Done"),
        )
        .child(quench_ui::view::element("progress").attr("aria-label", "Waiting"))
        .child(
            div()
                .attr("role", "slider")
                .attr("aria-valuemin", "1")
                .attr("aria-valuemax", "3")
                .attr("aria-valuenow", "7")
                .attr("aria-label", "Aria"),
        )
        .child(
            input()
                .attr("type", "text")
                .attr("aria-label", "Name")
                .on_input(move |v| *t.borrow_mut() = v.to_string()),
        );
    let (r, _m) = live(
        "body { margin: 0 } input, div { display: block; height: 20px; width: 100px }",
        view,
        300,
        300,
    );
    let tree = r.accessibility_tree();
    let all = flat(&tree);
    let by = |name: &str| {
        all.iter()
            .find(|n| n.name == name)
            .copied()
            .unwrap_or_else(|| {
                panic!(
                    "{name}: {:?}",
                    all.iter().map(|n| (&n.name, n.role)).collect::<Vec<_>>()
                )
            })
    };
    assert_eq!(
        (by("Level").role, by("Level").range),
        (Role::Slider, Some((10.0, 20.0, Some(12.0))))
    );
    assert_eq!(by("Level").value.as_deref(), Some("12"));
    assert_eq!(
        by("Middle").range,
        Some((0.0, 100.0, Some(50.0))),
        "a slider starts in the middle"
    );
    assert_eq!(
        (by("Done").role, by("Done").range),
        (Role::ProgressBar, Some((0.0, 1.0, Some(0.25))))
    );
    assert_eq!(by("Waiting").range, Some((0.0, 1.0, None)), "indeterminate");
    assert_eq!(
        by("Aria").range,
        Some((1.0, 3.0, Some(3.0))),
        "clamped into the range"
    );
    // A field's whole text is replaced, with an Input event, and the caret goes to its end.
    let field = by("Name").id;
    assert!(r.set_field_value(field, "Ann"));
    assert_eq!(*typed.borrow(), "Ann");
    assert_eq!(
        flat(&r.accessibility_tree())
            .iter()
            .find(|n| n.id == field)
            .unwrap()
            .caret,
        Some((3, 3))
    );
    assert!(r.set_field_value(field, "Bo"));
    assert_eq!(*typed.borrow(), "Bo");
    assert!(
        !r.set_field_value(by("Level").id, "x"),
        "only text fields take text"
    );
}

#[quench_test::test]
fn a_slider_moves_with_keys_and_the_pointer_and_shows_where_it_is() {
    let seen = Rc::new(std::cell::RefCell::new(Vec::new()));
    let s = seen.clone();
    let view = div()
        .child(
            input()
                .attr("type", "range")
                .attr("min", "0")
                .attr("max", "10")
                .attr("value", "5")
                .attr("aria-label", "Level")
                .on_input(move |v| s.borrow_mut().push(v.to_string())),
        )
        .child(
            quench_ui::view::element("progress")
                .attr("value", "0.5")
                .attr("aria-label", "Done"),
        );
    let (r, _m) = live(
        "body { margin: 0 } div { display: block } input[type=range] { width: 100px; height: 20px; margin: 0 } progress { display: block; width: 100px; height: 10px; border: 0; margin: 0; background: #000000 }",
        view,
        200,
        100,
    );
    let level = |r: &NativeRenderer| {
        flat(&r.accessibility_tree())
            .iter()
            .find(|n| n.name == "Level")
            .unwrap()
            .range
            .unwrap()
            .2
    };
    // Tab to it; the arrows, Page and Home/End move it by its step, a tenth of the range and to the ends.
    r.key_down(&key("Tab"));
    r.key_down(&key("ArrowRight"));
    assert_eq!(level(&r), Some(6.0));
    r.key_down(&key("ArrowLeft"));
    r.key_down(&key("ArrowLeft"));
    r.key_down(&key("PageUp"));
    assert_eq!(level(&r), Some(5.0));
    r.key_down(&key("End"));
    r.key_down(&key("ArrowRight"));
    assert_eq!(level(&r), Some(10.0), "kept in range");
    r.key_down(&key("Home"));
    assert_eq!(level(&r), Some(0.0));
    assert_eq!(seen.borrow().last().map(String::as_str), Some("0"));
    // The pointer sets the value under it, and dragging follows.
    r.pointer_down(75.0, 10.0);
    assert_eq!(
        level(&r),
        Some(8.0),
        "7.5 rounds to a whole step, as the default step of 1 asks"
    );
    r.pointer_move(25.0, 10.0);
    r.pointer_up(25.0, 10.0);
    assert_eq!(level(&r), Some(3.0), "dragged, then released");
    // The progress bar is half full: the left half is the accent, the right half the background.
    let mut canvas = Canvas::new(200, 100);
    r.render(&mut canvas);
    assert_eq!(canvas.pixel(20, 25), Color::rgb(0, 117, 255));
    assert_eq!(canvas.pixel(80, 25), Color::rgb(0, 0, 0));
}

#[quench_test::test]
fn a_transition_moves_a_value_gradually_and_keeps_frames_coming_until_it_is_done() {
    let on = create_signal(false);
    let flag = on.clone();
    let view = div().child(div().attr("id", "t").attr_fn("class", move || {
        if flag.get() {
            "after".to_string()
        } else {
            "before".to_string()
        }
    }));
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet(
        "body { margin: 0 } #t { width: 20px; height: 20px } \
         .before { opacity: 1; background: rgb(0, 0, 0); transition: opacity 1s linear, background-color 1s linear } \
         .after { opacity: 0; background: rgb(200, 100, 0); transition: opacity 1s linear, background-color 1s linear }",
    );
    r.set_viewport(40.0, 40.0);
    let _m = mount(&r, &r.root(), view);
    let mut canvas = Canvas::new(40, 40);
    let shown = |r: &NativeRenderer, canvas: &mut Canvas| {
        let boxes = r.render(canvas);
        let t = r.children(r.children(r.root())[0])[0];
        let b = boxes.find(t.index()).unwrap();
        (b.style.opacity, b.style.resolve(b.style.background))
    };
    let (opacity, background) = shown(&r, &mut canvas);
    assert_eq!(
        (opacity, background.r),
        (1.0, 0),
        "the first style is not transitioned to"
    );
    assert!(r.next_wake().is_none(), "nothing moves yet");
    on.set(true);
    let (opacity, background) = shown(&r, &mut canvas);
    assert_eq!(
        (opacity, background.r),
        (1.0, 0),
        "the change starts from what was shown"
    );
    assert!(
        r.next_wake()
            .is_some_and(|w| w <= std::time::Duration::from_millis(16)),
        "frames are wanted"
    );
    r.advance_animations(std::time::Duration::from_millis(500));
    assert!(r.tick(), "a step makes the page stale");
    let (opacity, background) = shown(&r, &mut canvas);
    assert!((opacity - 0.5).abs() < 0.05, "{opacity}");
    assert!((i32::from(background.r) - 100).abs() < 8, "{background:?}");
    // A change half way starts from where it is.
    on.set(false);
    let (reverse, _) = shown(&r, &mut canvas);
    assert!((reverse - 0.5).abs() < 0.05, "{reverse}");
    r.advance_animations(std::time::Duration::from_millis(2000));
    r.tick();
    let (done, background) = shown(&r, &mut canvas);
    assert_eq!((done, background.r), (1.0, 0));
    assert!(r.next_wake().is_none(), "finished: the page rests");
    // With reduced motion the change is immediate.
    r.set_media(quench_css::MediaEnv {
        reduced_motion: true,
        ..quench_css::MediaEnv::default()
    });
    on.set(true);
    let (instant, background) = shown(&r, &mut canvas);
    assert_eq!((instant, background.r), (0.0, 200));
}

#[quench_test::test]
fn keyframe_animations_play_repeat_alternate_fill_and_stop() {
    use std::time::Duration;
    let r = Rc::new(NativeRenderer::new());
    r.add_stylesheet(
        "body { margin: 0 } div { width: 10px; height: 10px } \
         @keyframes fade { from { opacity: 1 } to { opacity: 0 } } \
         @keyframes spin { 0% { transform: rotate(0deg) } 50% { background: rgb(0, 0, 0) } 100% { transform: rotate(360deg); background: rgb(200, 0, 0) } } \
         #a { animation: fade 1s linear infinite } \
         #b { animation: fade 1s linear 2 alternate forwards } \
         #c { animation-name: fade; animation-duration: 1s; animation-delay: 1s; animation-timing-function: linear; animation-fill-mode: backwards; opacity: 0.5 } \
         #d { animation: spin 2s linear infinite; background: rgb(100, 0, 0) } \
         #e { animation: missing 1s }",
    );
    r.set_viewport(100.0, 100.0);
    let view = div()
        .child(div().attr("id", "a"))
        .child(div().attr("id", "b"))
        .child(div().attr("id", "c"))
        .child(div().attr("id", "d"))
        .child(div().attr("id", "e"));
    let _m = mount(&r, &r.root(), view);
    let mut canvas = Canvas::new(100, 100);
    let styles = |r: &NativeRenderer, canvas: &mut Canvas| {
        let boxes = r.render(canvas);
        let wrapper = r.children(r.children(r.root())[0])[0];
        let _ = wrapper;
        let kids = r.children(r.children(r.root())[0]);
        kids.iter()
            .map(|k| boxes.find(k.index()).unwrap().style.clone())
            .collect::<Vec<_>>()
    };
    let at = |r: &NativeRenderer, canvas: &mut Canvas, ms: u64| {
        r.advance_animations(Duration::from_millis(ms));
        r.tick();
        styles(r, canvas)
    };
    let start = styles(&r, &mut canvas);
    assert_eq!(start[0].opacity, 1.0);
    assert_eq!(
        start[2].opacity, 1.0,
        "backwards fill: the first frame while it waits for its delay"
    );
    assert!(r.next_wake().is_some(), "frames are wanted");
    let half = at(&r, &mut canvas, 500);
    assert!((half[0].opacity - 0.5).abs() < 0.05, "{}", half[0].opacity);
    assert!((half[1].opacity - 0.5).abs() < 0.05);
    assert!(
        (half[3].transform.rotate - 90.0).abs() < 5.0,
        "a quarter of the way: {}",
        half[3].transform.rotate
    );
    let third = at(&r, &mut canvas, 1000);
    // 1.5 s: `a` is half way through its second run; `b` runs backwards in its second (alternate) run; `c` has
    // begun its first second; `d` is 3/4 of the way, past the middle keyframe.
    assert!((third[0].opacity - 0.5).abs() < 0.05);
    assert!(
        (third[1].opacity - 0.5).abs() < 0.05,
        "alternate: back up from 0 to 1 half way is 0.5"
    );
    assert!(
        (third[2].opacity - 0.5).abs() < 0.05,
        "c runs its own keyframes now: 1 to 0 at half way"
    );
    assert!(
        (third[3].transform.rotate - 270.0).abs() < 5.0,
        "{}",
        third[3].transform.rotate
    );
    assert_eq!(
        third[4].opacity, 1.0,
        "an animation with no keyframes does nothing"
    );
    // After two alternating runs (2 s) b stays at its end: forwards. `a` never ends.
    let late = at(&r, &mut canvas, 1000);
    assert_eq!(
        late[1].opacity, 1.0,
        "forwards: the last frame of the last run (it ran backwards)"
    );
    assert!(late[0].opacity <= 1.0);
    assert!(r.next_wake().is_some(), "`a` and `d` go on for ever");
    // Reduced motion shows the plain styles.
    r.set_media(quench_css::MediaEnv {
        reduced_motion: true,
        ..quench_css::MediaEnv::default()
    });
    let still = at(&r, &mut canvas, 100);
    assert_eq!((still[0].opacity, still[3].transform.rotate), (1.0, 0.0));
}

#[quench_test::test]
fn clicking_a_submit_button_submits_its_form_and_other_buttons_do_not() {
    let submitted = Rc::new(Cell::new(0));
    let s = submitted.clone();
    let view = div().child(
        quench_ui::view::element("form")
            .on_submit(move || s.set(s.get() + 1))
            .child(button().attr("aria-label", "plain").text("Go"))
            .child(
                button()
                    .attr("aria-label", "typed")
                    .attr("type", "button")
                    .text("Not"),
            )
            .child(
                button()
                    .attr("aria-label", "reset")
                    .attr("type", "reset")
                    .text("Reset"),
            )
            .child(input().attr("aria-label", "send").attr("type", "submit"))
            .child(input().attr("aria-label", "field").attr("type", "text")),
    );
    let (r, _m) = live(
        "body { margin: 0 } button, input { display: block; width: 100px; height: 20px }",
        view,
        200,
        200,
    );
    let click = |id: &str| {
        let all = r.accessibility_tree();
        let node = flat(&all).into_iter().find(|n| n.name == id).unwrap();
        let (x, y, w, h) = node.bounds;
        r.pointer_down(x + w / 2.0, y + h / 2.0);
        r.pointer_up(x + w / 2.0, y + h / 2.0);
    };
    click("plain");
    assert_eq!(submitted.get(), 1, "a button with no type submits");
    click("typed");
    click("reset");
    click("field");
    assert_eq!(
        submitted.get(),
        1,
        "buttons of other types and fields do not"
    );
    click("send");
    assert_eq!(submitted.get(), 2, "an input of type submit does");
}

#[quench_test::test]
fn a_long_press_selects_a_word_and_its_handles_can_be_moved() {
    let view = div().child(
        input()
            .attr("type", "text")
            .attr("aria-label", "t")
            .attr("value", "hello brave world, ok"),
    );
    let (r, _m) = live(
        "body { margin: 0 } input { display: block; width: 300px; height: 24px; font-size: 14px; padding: 0; border: 0 }",
        view,
        400,
        100,
    );
    let field = |r: &NativeRenderer| {
        flat(&r.accessibility_tree())
            .into_iter()
            .find(|n| n.name == "t")
            .unwrap()
            .clone()
    };
    let selection = |r: &NativeRenderer| {
        let (caret, anchor) = field(r).caret.unwrap();
        (caret.min(anchor), caret.max(anchor))
    };
    assert!(r.selection_handles().is_none(), "nothing is selected");
    assert!(
        !r.select_word_at(350.0, 90.0),
        "nothing to select where there is no field"
    );
    let (x, y, _, h) = field(&r).bounds;
    // The middle of "brave" is some way along the text: find it from where the caret goes for each character.
    let second_word_x = x + 6.0 * 7.0 + 12.0;
    assert!(r.select_word_at(second_word_x, y + h / 2.0));
    assert_eq!(selection(&r), (6, 11), "the word under the finger");
    let handles = r.selection_handles().unwrap();
    assert!(
        handles[0].0 < handles[1].0 && handles[0].2 > 0.0,
        "{handles:?}"
    );
    // Dragging the end handle to the right extends the selection; to the left of the start flips it.
    r.move_selection_end(true, handles[1].0 + 40.0);
    let (lo, hi) = selection(&r);
    assert_eq!(lo, 6);
    assert!(hi > 11, "{hi}");
    r.move_selection_end(true, handles[0].0 - 20.0);
    let (lo, hi) = selection(&r);
    assert!(
        lo < 6 && hi == 6,
        "the dragged end went past the other: {lo} {hi}"
    );
    // A space or punctuation selects itself; the end of the text and elsewhere does not break.
    assert!(r.select_word_at(x + 300.0 - 2.0, y + h / 2.0));
    assert!(selection(&r).1 >= selection(&r).0);
    r.move_selection_end(false, x - 50.0);
    assert_eq!(selection(&r).0, 0);
}

#[quench_test::test]
fn a_scroll_or_a_blink_reuses_the_layout_but_a_hover_or_a_change_makes_a_new_one() {
    let view = div()
        .child(
            div()
                .attr("id", "scroller")
                .child(div().style("height", "300px").text("tall")),
        )
        .child(button().class("b"));
    let css = "body { margin: 0 } #scroller { height: 100px; width: 100px; overflow: auto } button { display: block; border: 0; padding: 0; width: 20px; height: 20px; background: white } button:hover { background: red }";
    let (r, _m) = live(css, view, 200, 200);
    let mut canvas = Canvas::new(200, 200);
    let first = r.render(&mut canvas);
    // Nothing changed at all: the same layout.
    assert!(Rc::ptr_eq(&first, &r.render(&mut canvas)));
    // A scroll changes where things are drawn, not where they are laid out.
    r.wheel(10.0, 10.0, 0.0, 60.0);
    assert!(r.take_dirty(), "the frame is redrawn");
    let scrolled = r.render(&mut canvas);
    assert!(Rc::ptr_eq(&first, &scrolled), "a scroll reuses the layout");
    // So does the blink of the caret.
    r.set_caret_visible(false);
    assert!(Rc::ptr_eq(&first, &r.render(&mut canvas)));
    // A pointer over a button with a :hover rule restyles, so the layout is new - and the button is red.
    r.pointer_move(10.0, 110.0);
    let hovered = r.render(&mut canvas);
    assert!(!Rc::ptr_eq(&first, &hovered), "a hover is laid out again");
    assert_eq!(canvas.pixel(5, 105), RED);
}

/// The rows of `canvas` that have any red in them (the caret and glyphs of a `color: red` field).
fn red_rows(canvas: &Canvas) -> (usize, usize) {
    let rows: Vec<usize> = (0..canvas.height())
        .filter(|y| {
            (0..canvas.width()).any(|x| canvas.pixel(x, *y).r > 200 && canvas.pixel(x, *y).g < 80)
        })
        .collect();
    (*rows.first().unwrap(), *rows.last().unwrap())
}

#[quench_test::test]
fn the_caret_of_an_empty_field_stands_where_the_text_will_be() {
    // A field as an app draws it: the kit's own style (a touch target of 48 px at least), the text in the middle,
    // on a screen of 1x and on a phone's 2.8x.
    for scale in [1.0_f32, 2.8125] {
        let view = div()
            .class("q-textfield")
            .child(input().attr("type", "text").class("q-textfield-input"));
        let css = format!(
            "{}{}{}body {{ margin: 0 }} .q-textfield .q-textfield-input {{ padding: 0; background: white; color: red; border: 0; border-bottom: 1px solid #525252; font-size: 16px }}",
            quench_style::tokens::stylesheet(),
            quench_style::mobile::stylesheet(),
            quench_style::components::stylesheet()
        );
        let r = Rc::new(NativeRenderer::new());
        r.add_stylesheet(&css);
        r.set_scale(scale);
        let (w, h) = ((220.0 * scale) as usize, (80.0 * scale) as usize);
        r.set_viewport(w as f32, h as f32);
        let _m = mount(&r, &r.root(), view);
        let mut canvas = Canvas::new(w, h);
        r.render(&mut canvas);
        let field = r.children(r.children(r.root())[0])[0];
        r.pointer_down(10.0 * scale, 20.0 * scale);
        r.pointer_up(10.0 * scale, 20.0 * scale);
        let mut empty = Canvas::new(w, h);
        r.render(&mut empty);
        let caret_empty = red_rows(&empty);
        // A space has no ink, so what is red now is the caret after it: where it is once there is text.
        r.text(" ");
        assert_eq!(r.attr(field, "value").as_deref(), Some(" "));
        let mut typed = Canvas::new(w, h);
        r.render(&mut typed);
        let caret_typed = red_rows(&typed);
        assert_eq!(
            caret_empty, caret_typed,
            "scale {scale}: the empty caret {caret_empty:?} and the one after text {caret_typed:?} are at the same height"
        );
    }
}
