//! The DOM renderer driven through the in-memory host.

use quench_ui::prelude::*;
use quench_ui_web::mock::{self, Call};
use quench_ui_web::{DomRenderer, mount_root};

fn counter() -> (View, Signal<i32>) {
    let count = create_signal(0);
    let (shown, up) = (count.clone(), count.clone());
    let view = div()
        .child(span().text_fn(move || format!("count: {}", shown.get())))
        .child(button().text("+").on_click(move || up.update(|c| *c += 1)))
        .into_view();
    (view, count)
}

#[quench_test::test]
fn the_counter_builds_and_patches_through_the_host() {
    mock::reset();
    let (view, _) = counter();
    let m = mount_root(view);
    assert_eq!(
        mock::html(),
        "<div><span>count: 0</span><button>+</button></div>"
    );
    mock::clear_calls();
    let button = mock::handles_by_tag("button")[0];
    assert!(mock::fire_click(button));
    assert!(mock::fire_click(button));
    let calls = mock::calls();
    let patches = calls
        .iter()
        .filter(|c| matches!(c, Call::SetText(..)))
        .count();
    assert_eq!(
        patches, 2,
        "one text patch per click, nothing else: {calls:?}"
    );
    assert_eq!(calls.len(), 2);
    assert_eq!(mock::text(), "count: 2+");
    m.unmount();
}

#[quench_test::test]
fn unmounting_releases_every_handle_and_listener() {
    mock::reset();
    let (view, _) = counter();
    let m = mount_root(view);
    assert!(mock::live_handles() >= 4);
    assert_eq!(mock::live_listeners(), 1);
    m.unmount();
    assert_eq!(mock::live_handles(), 0);
    assert_eq!(mock::live_listeners(), 0);
    assert_eq!(mock::html(), "");
}

#[quench_test::test]
fn released_handles_are_reused_and_the_root_is_never_released() {
    mock::reset();
    let r = DomRenderer;
    let a = r.create_element("div");
    let first = a.id();
    assert!(first >= 2, "0 is no node and 1 is the root");
    drop(a);
    let b = r.create_element("span");
    assert_eq!(b.id(), first, "the freed slot is reused");
    let root = DomRenderer::root();
    assert_eq!(root.id(), 1);
    drop(root);
    assert!(!mock::calls().contains(&Call::Release(1)));
    assert!(format!("{b:?}").starts_with("Node("));
    assert!(format!("{DomRenderer:?}").contains("DomRenderer"));
}

#[quench_test::test]
fn ten_thousand_nodes_come_and_go_without_growing_the_handle_table() {
    mock::reset();
    let items = create_signal((0..10_000_u32).collect::<Vec<_>>());
    let list = items.clone();
    let m = mount_root(ul().child(View::keyed(
        move || list.get(),
        |i| *i,
        |i| li().text(i.to_string()).into_view(),
    )));
    let busy = mock::live_handles();
    assert!(busy > 20_000);
    items.set(Vec::new());
    let settled = mock::live_handles();
    assert_eq!(settled, 2, "the ul and the list's anchor");
    items.set((0..10_000).collect());
    assert_eq!(
        mock::live_handles(),
        busy,
        "the same number again, not more"
    );
    m.unmount();
    assert_eq!(mock::live_handles(), 0);
}

#[quench_test::test]
fn dropping_a_mount_releases_static_handles_but_leaves_the_dom() {
    mock::reset();
    let m = mount_root(div().child(p().text("kept")));
    assert!(mock::live_handles() > 0);
    drop(m);
    assert_eq!(mock::live_handles(), 0);
    assert_eq!(
        mock::html(),
        "<div><p>kept</p></div>",
        "the page is untouched"
    );
}

#[quench_test::test]
fn a_leaked_mount_stays_live() {
    mock::reset();
    let (view, _) = counter();
    mount_root(view).leak();
    assert!(mock::live_handles() > 0);
    assert!(mock::fire_click(mock::handles_by_tag("button")[0]));
    assert_eq!(mock::text(), "count: 1+");
}

#[quench_test::test]
fn listeners_register_once_per_kind_and_all_handlers_run() {
    mock::reset();
    let hits = std::rc::Rc::new(std::cell::Cell::new(0));
    let (h1, h2, h3) = (hits.clone(), hits.clone(), hits.clone());
    let m = mount_root(
        button()
            .on_click(move || h1.set(h1.get() + 1))
            .on_click(move || h2.set(h2.get() + 10))
            .on(EventKind::Focus, move |_| h3.set(h3.get() + 100)),
    );
    let listens = mock::calls()
        .into_iter()
        .filter(|c| matches!(c, Call::Listen(..)))
        .count();
    assert_eq!(
        listens, 2,
        "one host listener per kind, however many handlers"
    );
    let b = mock::handles_by_tag("button")[0];
    mock::fire_click(b);
    assert_eq!(hits.get(), 11);
    assert!(mock::fire(b, 7, "", 0.0, 0.0, 0, 0));
    assert_eq!(hits.get(), 111);
    assert!(
        !mock::fire(b, 8, "", 0.0, 0.0, 0, 0),
        "no blur listener, so the host sends nothing"
    );
    m.unmount();
}

#[quench_test::test]
fn form_and_attribute_calls_reach_the_host() {
    mock::reset();
    let value = create_signal("a".to_string());
    let v = value.clone();
    let m = mount_root(
        input()
            .attr("type", "text")
            .class("wide")
            .attr_fn("value", move || v.get()),
    );
    let i = mock::handles_by_tag("input")[0];
    assert_eq!(mock::attr(i, "value").as_deref(), Some("a"));
    assert_eq!(mock::attr(i, "class").as_deref(), Some("wide"));
    value.set("b".into());
    assert_eq!(mock::attr(i, "value").as_deref(), Some("b"));
    m.unmount();
}

#[quench_test::test]
fn calls_with_unknown_handles_are_ignored() {
    mock::reset();
    let r = DomRenderer;
    let real = r.create_element("div");
    let ghost = {
        let n = r.create_element("span");
        let id = n.id();
        drop(n);
        id
    };
    // Dispatching to a released, an unknown and a zero handle does nothing and does not panic.
    for h in [ghost, 9_999, 0] {
        assert!(!mock::fire_click(h));
    }
    let _ = real;
    assert_eq!(mock::html(), "");
}

#[quench_test::test]
fn the_panic_hook_sends_the_message_to_the_host() {
    mock::reset();
    quench_ui_web::install_panic_hook();
    let result = std::panic::catch_unwind(|| panic!("something broke"));
    let _ = std::panic::take_hook();
    assert!(result.is_err());
    let logs = mock::logs();
    assert!(
        logs.iter()
            .any(|(level, text)| *level == 3 && text.contains("something broke")),
        "{logs:?}"
    );
}

#[quench_test::test]
fn run_installs_the_hook_and_mounts_the_app() {
    mock::reset();
    quench_ui_web::run(|| p().text("hello"));
    let _ = std::panic::take_hook();
    assert_eq!(mock::html(), "<p>hello</p>");
    assert!(quench_ui_web::QUENCH_JS.contains("export async function start"));
    assert!(
        quench_ui_web::QUENCH_JS.lines().count() < 300,
        "quench.js stays under 300 lines"
    );
}

#[quench_test::test]
fn the_todo_flow_keeps_order_and_nodes_in_the_mock_dom() {
    mock::reset();
    let ids = create_signal(vec![1_u32, 2, 3, 4]);
    let list = ids.clone();
    let m = mount_root(ul().child(View::keyed(
        move || list.get(),
        |i| *i,
        |i| li().text(i.to_string()).into_view(),
    )));
    assert_eq!(mock::text(), "1234");
    ids.set(vec![4, 2, 3, 1, 5]);
    assert_eq!(mock::text(), "42315");
    ids.set(vec![5, 1]);
    assert_eq!(mock::text(), "51");
    m.unmount();
    assert_eq!(mock::live_handles(), 0);
}

#[quench_test::test]
fn run_hydrates_when_the_page_arrived_with_content_and_builds_when_it_did_not() {
    mock::reset();
    mock::set_server_content(true, 0);
    quench_ui_web::run(|| p().text("hello"));
    let _ = std::panic::take_hook();
    let calls = mock::calls();
    let begin = calls
        .iter()
        .position(|c| *c == Call::HydrateBegin)
        .expect("hydration started");
    let end = calls
        .iter()
        .position(|c| *c == Call::HydrateEnd)
        .expect("and ended");
    let create = calls
        .iter()
        .position(|c| matches!(c, Call::CreateElement(..)))
        .unwrap();
    assert!(
        begin < create && create < end,
        "the view is built between begin and end: {calls:?}"
    );

    mock::reset();
    quench_ui_web::run(|| p().text("hello"));
    let _ = std::panic::take_hook();
    assert!(
        !mock::calls().contains(&Call::HydrateBegin),
        "no server content, so a plain mount"
    );
}

#[quench_test::test]
fn hydrate_root_reports_how_many_nodes_did_not_match() {
    mock::reset();
    mock::set_server_content(true, 3);
    let (mounted, mismatches) = quench_ui_web::hydrate_root(div().child(p().text("x")));
    assert_eq!(mismatches, 3);
    mock::set_server_content(true, 0);
    let (other, clean) = quench_ui_web::hydrate_root(span());
    assert_eq!(clean, 0);
    mounted.unmount();
    other.unmount();
}
