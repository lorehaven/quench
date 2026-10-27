//! Views mounted into a `TestRenderer`: builders, fine-grained updates, events, unmounting.

use quench_ui::prelude::*;
use quench_ui::test_renderer::Op;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn setup() -> (Rc<TestRenderer>, quench_ui::test_renderer::NodeId) {
    let r = Rc::new(TestRenderer::new());
    let root = r.root();
    (r, root)
}

#[quench_test::test]
fn static_views_build_the_expected_html() {
    let (r, root) = setup();
    let m = mount(
        &r,
        &root,
        div()
            .class("card")
            .class("wide")
            .attr("id", "a")
            .attr("id", "b") // replaces
            .style("color", "red")
            .style("margin", "1px")
            .style("color", "blue")
            .child(h1().text("Title"))
            .child(ul().children((1..=2).map(|n| li().text(format!("item {n}")).into_view())))
            .child(input().attr("type", "text"))
            .child(()),
    );
    assert_eq!(
        r.html(),
        "<div id=\"b\" class=\"card wide\" style=\"color: blue; margin: 1px\"><h1>Title</h1>\
         <ul><li>item 1</li><li>item 2</li></ul><input type=\"text\"></div>"
    );
    assert!(format!("{m:?}").contains("Mount"));
    assert!(!m.scope().is_disposed());
    m.unmount();
    assert_eq!(r.html(), "");
}

#[quench_test::test]
fn into_view_covers_the_common_types() {
    let (r, root) = setup();
    let _ = mount(
        &r,
        &root,
        fragment([
            1_i32.into_view(),
            "-".into_view(),
            String::from("s").into_view(),
            2.5_f64.into_view(),
            true.into_view(),
            Some("o").into_view(),
            None::<&str>.into_view(),
            vec!["a", "b"].into_view(),
            View::text("t"),
            View::Empty,
        ]),
    );
    assert_eq!(r.text(), "1-s2.5trueoabt");
    assert!(format!("{:?}", span().text("x").into_view()).contains("Element"));
    assert!(format!("{:?}", View::text_fn(String::new)).contains("DynText"));
    assert!(format!("{:?}", View::dynamic(|| View::Empty)).contains("Dynamic"));
    assert!(format!("{:?}", View::Empty).contains("Empty"));
    assert!(format!("{:?}", View::Fragment(vec![])).contains("Fragment"));
    assert!(
        format!(
            "{:?}",
            View::keyed(Vec::<i32>::new, |i| *i, |_| View::Empty)
        )
        .contains("Keyed")
    );
    assert!(format!("{:?}", View::text("x")).contains("Text"));
}

#[quench_test::test]
fn the_counter_updates_only_the_text_node() {
    let (r, root) = setup();
    let count = create_signal(0);
    let (c1, c2) = (count.clone(), count.clone());
    let m = mount(
        &r,
        &root,
        div()
            .child(span().text_fn(move || format!("count: {}", c1.get())))
            .child(button().text("+").on_click(move || c2.update(|c| *c += 1))),
    );
    assert_eq!(
        r.html(),
        "<div><span>count: 0</span><button>+</button></div>"
    );
    r.clear_ops();
    let button_node = r.find_by_tag("button")[0];
    r.fire(button_node, &Event::Click);
    r.fire(button_node, &Event::Click);
    let span_node = r.find_by_tag("span")[0];
    let text_node = r.children(span_node)[0];
    assert_eq!(
        r.ops(),
        [
            Op::SetText(text_node, "count: 1".into()),
            Op::SetText(text_node, "count: 2".into())
        ],
        "each click is exactly one text patch"
    );
    assert_eq!(r.text_of(span_node), "count: 2");
    m.unmount();
}

#[quench_test::test]
fn several_writes_in_one_handler_make_one_patch() {
    let (r, root) = setup();
    let (a, b) = (create_signal(1), create_signal(1));
    let (a1, b1, a2, b2) = (a.clone(), b.clone(), a.clone(), b.clone());
    let m = mount(
        &r,
        &root,
        div()
            .child(p().text_fn(move || format!("{}x{}", a1.get(), b1.get())))
            .child(button().on_click(move || {
                a2.set(5);
                b2.set(6);
            })),
    );
    r.clear_ops();
    r.fire(r.find_by_tag("button")[0], &Event::Click);
    let patches = r
        .ops()
        .into_iter()
        .filter(|o| matches!(o, Op::SetText(..)))
        .count();
    assert_eq!(patches, 1);
    assert_eq!(r.text(), "5x6");
    m.unmount();
}

#[quench_test::test]
fn attributes_classes_and_booleans_follow_signals() {
    let (r, root) = setup();
    let on = create_signal(false);
    let title = create_signal(Some("hi".to_string()));
    let (o1, o2, o3, t1) = (on.clone(), on.clone(), on.clone(), title.clone());
    let m = mount(
        &r,
        &root,
        div()
            .class("base")
            .class_if("active", move || o1.get())
            .attr_fn("data-state", move || {
                if o2.get() { "on" } else { "off" }.to_string()
            })
            .attr_bool("hidden", move || !o3.get())
            .attr_opt("title", move || t1.get()),
    );
    let d = r.find_by_tag("div")[0];
    let attrs = |r: &TestRenderer| ["class", "data-state", "hidden", "title"].map(|n| r.attr(d, n));
    assert_eq!(
        attrs(&r),
        [
            Some("base".into()),
            Some("off".into()),
            Some(String::new()),
            Some("hi".into())
        ]
    );
    on.set(true);
    title.set(None);
    assert_eq!(
        attrs(&r),
        [Some("base active".into()), Some("on".into()), None, None]
    );
    m.unmount();
}

#[quench_test::test]
fn a_dynamic_region_rebuilds_in_place_between_its_siblings() {
    let (r, root) = setup();
    let which = create_signal(0);
    let w = which.clone();
    let m = mount(
        &r,
        &root,
        div()
            .child(span().text("before"))
            .child(View::dynamic(move || match w.get() {
                0 => p().text("zero").into_view(),
                1 => fragment([b'x'.into_view(), "one".into_view()]),
                _ => View::Empty,
            }))
            .child(span().text("after")),
    );
    let inner = |r: &TestRenderer| r.html().replace("<!---->", "");
    assert_eq!(
        inner(&r),
        "<div><span>before</span><p>zero</p><span>after</span></div>"
    );
    which.set(1);
    assert_eq!(
        inner(&r),
        "<div><span>before</span>120one<span>after</span></div>"
    );
    which.set(2);
    assert_eq!(
        inner(&r),
        "<div><span>before</span><span>after</span></div>"
    );
    which.set(0);
    assert_eq!(
        inner(&r),
        "<div><span>before</span><p>zero</p><span>after</span></div>"
    );
    m.unmount();
    assert_eq!(r.live_nodes(), 1);
}

#[quench_test::test]
fn show_adds_and_removes_and_disposes_what_it_hid() {
    let (r, root) = setup();
    let visible = create_signal(true);
    let ticks = create_signal(0);
    let (v, t) = (visible.clone(), ticks.clone());
    let inner_runs = Rc::new(Cell::new(0));
    let ir = inner_runs.clone();
    let m = mount(
        &r,
        &root,
        View::show(
            move || v.get(),
            move || {
                let (t, ir) = (t.clone(), ir.clone());
                p().text_fn(move || {
                    ir.set(ir.get() + 1);
                    t.get().to_string()
                })
                .into_view()
            },
        ),
    );
    assert_eq!(r.text(), "0");
    visible.set(false);
    assert_eq!(r.text(), "");
    ticks.set(1); // the hidden effect is gone, so this must not run it
    assert_eq!(inner_runs.get(), 1);
    visible.set(true);
    assert_eq!(r.text(), "1");
    m.unmount();
}

#[quench_test::test]
fn events_reach_their_listeners_with_their_payload() {
    let (r, root) = setup();
    let seen = Rc::new(RefCell::new(Vec::<String>::new()));
    let (s1, s2, s3, s4, s5, s6) = (
        seen.clone(),
        seen.clone(),
        seen.clone(),
        seen.clone(),
        seen.clone(),
        seen.clone(),
    );
    let m = mount(
        &r,
        &root,
        input()
            .on_input(move |v| s1.borrow_mut().push(format!("input:{v}")))
            .on_change(move |v| s2.borrow_mut().push(format!("change:{v}")))
            .on_submit(move || s3.borrow_mut().push("submit".into()))
            .on_keydown(move |k| s4.borrow_mut().push(format!("key:{}", k.key)))
            .on(EventKind::PointerDown, move |e| {
                if let Event::PointerDown(p) = e {
                    s5.borrow_mut().push(format!("ptr:{}", p.x));
                }
            })
            .on(EventKind::Focus, move |_| {
                s6.borrow_mut().push("focus".into())
            }),
    );
    let n = r.find_by_tag("input")[0];
    r.fire(n, &Event::Input("a".into()));
    r.fire(n, &Event::Change("b".into()));
    r.fire(n, &Event::Submit);
    r.fire(n, &Event::KeyDown(KeyEvent::new("Enter")));
    r.fire(
        n,
        &Event::PointerDown(PointerEvent {
            x: 3.0,
            ..PointerEvent::default()
        }),
    );
    r.fire(n, &Event::Focus);
    r.fire(n, &Event::Blur); // no listener
    r.fire(r.root(), &Event::Click); // a node without listeners
    assert_eq!(
        *seen.borrow(),
        [
            "input:a",
            "change:b",
            "submit",
            "key:Enter",
            "ptr:3",
            "focus"
        ]
    );
    m.unmount();
}

#[quench_test::test]
fn event_kinds_and_names_are_consistent() {
    let events = [
        (Event::Click, "click"),
        (Event::DoubleClick, "dblclick"),
        (Event::Input(String::new()), "input"),
        (Event::Change(String::new()), "change"),
        (Event::KeyDown(KeyEvent::default()), "keydown"),
        (Event::KeyUp(KeyEvent::default()), "keyup"),
        (Event::Submit, "submit"),
        (Event::Focus, "focus"),
        (Event::Blur, "blur"),
        (Event::PointerDown(PointerEvent::default()), "pointerdown"),
        (Event::PointerUp(PointerEvent::default()), "pointerup"),
        (Event::PointerMove(PointerEvent::default()), "pointermove"),
    ];
    for (event, name) in events {
        assert_eq!(event.kind().dom_name(), name);
    }
}

#[quench_test::test]
fn unmounting_removes_nodes_listeners_and_releases_captures() {
    let (r, root) = setup();
    let payload = Rc::new(());
    let s = create_signal(0);
    let (p, s1, s2) = (payload.clone(), s.clone(), s.clone());
    let m = mount(
        &r,
        &root,
        div()
            .child(span().text_fn(move || {
                let _ = &p;
                s1.get().to_string()
            }))
            .child(button().on_click(move || s2.set(1)))
            .child(View::dynamic({
                let s = s.clone();
                move || span().text(s.get().to_string()).into_view()
            })),
    );
    assert!(Rc::strong_count(&payload) > 1);
    assert!(r.live_nodes() > 4);
    assert_eq!(r.live_listeners(), 1);
    m.unmount();
    assert_eq!(r.live_nodes(), 1);
    assert_eq!(r.live_listeners(), 0);
    assert_eq!(Rc::strong_count(&payload), 1);
    r.clear_ops();
    s.set(9); // nothing is left to react
    assert!(r.ops().is_empty());
}

#[quench_test::test]
fn mounting_and_unmounting_ten_thousand_nodes_returns_to_baseline() {
    let (r, root) = setup();
    let items = create_signal((0..10_000).collect::<Vec<i32>>());
    let list = items.clone();
    let m = mount(
        &r,
        &root,
        ul().child(View::keyed(
            move || list.get(),
            |i| *i,
            |i| li().text(i.to_string()).into_view(),
        )),
    );
    assert!(r.live_nodes() > 20_000);
    items.set(Vec::new());
    assert_eq!(r.live_nodes(), 3, "root, ul and the list anchor");
    m.unmount();
    assert_eq!(r.live_nodes(), 1);
}

#[quench_test::test]
fn mismatched_event_delivery_is_caught_in_debug_builds() {
    // `fire` only hands a listener events of its own kind, so a wrapper never sees a mismatch.
    let (r, root) = setup();
    let hits = Rc::new(Cell::new(0));
    let h = hits.clone();
    let m = mount(
        &r,
        &root,
        button().on(EventKind::Click, move |_| h.set(h.get() + 1)),
    );
    let b = r.find_by_tag("button")[0];
    r.fire(b, &Event::DoubleClick);
    r.fire(b, &Event::Click);
    assert_eq!(hits.get(), 1);
    m.unmount();
}

#[quench_test::test]
fn the_view_a_dynamic_closure_builds_can_hold_more_dynamic_parts() {
    let (r, root) = setup();
    let outer = create_signal(1);
    let inner = create_signal("a".to_string());
    let (o, i) = (outer.clone(), inner.clone());
    let m = mount(
        &r,
        &root,
        View::dynamic(move || {
            let (n, i) = (o.get(), i.clone());
            div()
                .class_if("hot", move || n > 1)
                .child(View::text_fn(move || format!("{n}{}", i.get())))
                .into_view()
        }),
    );
    assert_eq!(r.text(), "1a");
    inner.set("b".into());
    assert_eq!(r.text(), "1b");
    outer.set(2);
    assert_eq!(r.text(), "2b");
    assert_eq!(
        r.attr(r.find_by_tag("div")[0], "class").as_deref(),
        Some("hot")
    );
    inner.set("c".into());
    assert_eq!(r.text(), "2c");
    m.unmount();
}

#[quench_test::test]
fn a_view_can_use_context_provided_above_it() {
    let (r, root) = setup();
    let (m, scope) = create_root(|| {
        provide_context("dark".to_string());
        mount(
            &r,
            &root,
            View::dynamic(|| {
                span()
                    .text(use_context::<String>().unwrap_or_default())
                    .into_view()
            }),
        )
    });
    assert_eq!(r.text(), "dark");
    m.unmount();
    scope.dispose();
}

#[quench_test::test]
#[should_panic(expected = "invalid tag name")]
fn element_rejects_a_tag_that_could_break_out_of_markup() {
    let _ = element("div onload=x");
}

#[quench_test::test]
fn the_todo_list_adds_toggles_and_removes_items() {
    #[derive(Clone)]
    struct Todo {
        id: u32,
        title: String,
        done: Signal<bool>,
    }
    let (r, root) = setup();
    let todos = create_signal(Vec::<Todo>::new());
    let next_id = Rc::new(Cell::new(1_u32));
    let draft = create_signal(String::new());

    let add = {
        let (todos, draft, next_id) = (todos.clone(), draft.clone(), next_id.clone());
        move || {
            let title = draft.get_untracked();
            if title.is_empty() {
                return;
            }
            let id = next_id.get();
            next_id.set(id + 1);
            todos.update(|t| {
                t.push(Todo {
                    id,
                    title,
                    done: create_signal(false),
                })
            });
            draft.set(String::new());
        }
    };
    let list = todos.clone();
    let list_for_count = todos.clone();
    let todos_for_remove = todos.clone();
    let draft_for_input = draft.clone();
    let m = mount(
        &r,
        &root,
        div()
            .child(
                input()
                    .attr("placeholder", "what next?")
                    .on_input(move |v| draft_for_input.set(v.to_string())),
            )
            .child(button().text("Add").on_click(add))
            .child(p().text_fn(move || {
                format!(
                    "{} left",
                    list_for_count.with(|t| t.iter().filter(|x| !x.done.get()).count())
                )
            }))
            .child(ul().child(View::keyed(
                move || list.get(),
                |t| t.id,
                move |t| {
                    let (done, done2) = (t.done.clone(), t.done.clone());
                    let (todos, id) = (todos_for_remove.clone(), t.id);
                    li().class_if("done", move || done.get())
                        .child(span().text(t.title))
                        .child(
                            button()
                                .text("toggle")
                                .on_click(move || done2.update(|d| *d = !*d)),
                        )
                        .child(
                            button()
                                .text("x")
                                .on_click(move || todos.update(|l| l.retain(|x| x.id != id))),
                        )
                        .into_view()
                },
            ))),
    );
    let type_and_add = |text: &str| {
        let input_node = r.find_by_tag("input")[0];
        r.fire(input_node, &Event::Input(text.to_string()));
        r.fire(r.find_by_tag("button")[0], &Event::Click);
    };
    let titles = |r: &TestRenderer| -> Vec<String> {
        r.find_by_tag("li")
            .iter()
            .map(|n| r.text_of(r.children(*n)[0]))
            .collect()
    };
    assert_eq!(r.text_of(r.find_by_tag("p")[0]), "0 left");

    type_and_add("milk");
    type_and_add("eggs");
    type_and_add("bread");
    assert_eq!(titles(&r), ["milk", "eggs", "bread"]);
    assert_eq!(r.text_of(r.find_by_tag("p")[0]), "3 left");

    // toggle the second item: its class changes, the others and the node identities do not
    let eggs_li = r.find_by_tag("li")[1];
    r.clear_ops();
    let toggle = r
        .find_by_tag("button")
        .into_iter()
        .filter(|b| r.text_of(*b) == "toggle")
        .nth(1)
        .unwrap();
    r.fire(toggle, &Event::Click);
    assert_eq!(r.attr(eggs_li, "class").as_deref(), Some("done"));
    assert_eq!(r.attr(r.find_by_tag("li")[0], "class"), None);
    assert_eq!(r.text_of(r.find_by_tag("p")[0]), "2 left");
    assert!(
        r.ops()
            .iter()
            .all(|op| matches!(op, Op::SetAttr(..) | Op::RemoveAttr(..) | Op::SetText(..))),
        "toggling creates and moves no nodes: {:?}",
        r.ops()
    );

    // remove the first item; the survivors keep their nodes
    let survivors = r.find_by_tag("li")[1..].to_vec();
    let remove = r
        .find_by_tag("button")
        .into_iter()
        .find(|b| r.text_of(*b) == "x")
        .unwrap();
    r.fire(remove, &Event::Click);
    assert_eq!(titles(&r), ["eggs", "bread"]);
    assert_eq!(r.find_by_tag("li"), survivors);

    m.unmount();
    assert_eq!(r.live_nodes(), 1);
}

#[quench_test::test]
fn nodes_are_created_in_document_order() {
    // Dynamic and keyed regions nested in each other and in static markup: whatever the shape, the
    // order nodes are created in is the order they appear in the document.
    let (r, root) = setup();
    let items = create_signal(vec![1_u32, 2, 3]);
    let flag = create_signal(true);
    let (list, shown) = (items.clone(), flag.clone());
    let m = mount(
        &r,
        &root,
        div()
            .child("start")
            .child(View::dynamic(move || {
                let list = list.clone();
                if shown.get() {
                    fragment([
                        span().text("a").into_view(),
                        View::keyed(
                            move || list.get(),
                            |i| *i,
                            |i| {
                                li().text(i.to_string())
                                    .child(View::dynamic(move || format!("-{i}").into_view()))
                                    .into_view()
                            },
                        ),
                        View::text_fn(|| "tail".to_string()),
                    ])
                } else {
                    View::Empty
                }
            }))
            .child(p().text("end")),
    );
    let order = r.document_order();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
    // Still true after the lists change and are rebuilt.
    items.set(vec![3, 1, 4, 5]);
    flag.set(false);
    flag.set(true);
    assert_eq!(r.text(), "starta3-31-14-45-5tailend");
    m.unmount();
}

#[quench_test::test]
fn a_newline_straight_after_pre_or_textarea_survives_html_parsing() {
    let html = quench_ui::render_to_string(
        div()
            .child(pre().text("\nfirst"))
            .child(textarea().text("\nsecond"))
            .child(pre().text("no newline")),
    );
    // The parser drops one newline after <pre> and <textarea>, so the serialiser writes two.
    assert_eq!(
        html,
        "<div><pre>\n\nfirst</pre><textarea>\n\nsecond</textarea><pre>no newline</pre></div>"
    );
}
