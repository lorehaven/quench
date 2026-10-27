//! The virtual list builds the rows in view and follows scrolling and resizing.

use quench_ui::prelude::*;
use quench_ui::virtual_list::{OVERSCAN, virtual_list};
use quench_ui::{ScrollEvent, Size};
use std::rc::Rc;

struct Setup {
    r: Rc<TestRenderer>,
    list: quench_ui::test_renderer::NodeId,
    items: Signal<Vec<u32>>,
    _mount: Mount<TestRenderer>,
}

fn setup(count: u32) -> Setup {
    let r = Rc::new(TestRenderer::new());
    let items = create_signal((0..count).collect::<Vec<_>>());
    let view = virtual_list(
        &items,
        40.0,
        |n| u64::from(*n),
        |n| span().text(format!("row {n}")).into_view(),
    );
    let mount = mount(&r, &r.root(), view);
    let list = r.children(r.root())[0];
    Setup {
        r,
        list,
        items,
        _mount: mount,
    }
}

fn rows(s: &Setup) -> Vec<String> {
    s.r.find_by_tag("span")
        .into_iter()
        .map(|n| s.r.text_of(n))
        .collect()
}

fn first_row(s: &Setup) -> u32 {
    rows(s)[0].trim_start_matches("row ").parse().unwrap()
}

#[quench_test::test]
fn only_the_rows_in_view_are_built() {
    let s = setup(10_000);
    // 640 / 40 = 16 rows in view, one more for a partial row, and the overscan below.
    assert_eq!(rows(&s).len(), 16 + 1 + OVERSCAN);
    assert_eq!(first_row(&s), 0);
    let spacer = s.r.children(s.list)[0];
    assert_eq!(s.r.style_of(spacer, "height").as_deref(), Some("400000px"));
    let window = s.r.children(spacer)[0];
    assert_eq!(s.r.style_of(window, "--q-row-h").as_deref(), Some("40px"));
    assert_eq!(
        s.r.style_of(window, "transform").as_deref(),
        Some("translateY(0px)")
    );
    assert_eq!(s.r.attr(window, "role").as_deref(), Some("list"));
    assert_eq!(s.r.find_by_attr("role", "listitem").len(), rows(&s).len());
}

#[quench_test::test]
fn scrolling_moves_the_window_and_builds_only_what_came_into_view() {
    let s = setup(10_000);
    let window = s.r.children(s.r.children(s.list)[0])[0];
    s.r.fire(
        s.list,
        &Event::Scroll(ScrollEvent {
            top: 4000.0,
            left: 0.0,
        }),
    );
    assert_eq!(first_row(&s), 100 - OVERSCAN as u32);
    assert_eq!(rows(&s).len(), 16 + 1 + 2 * OVERSCAN);
    assert_eq!(
        s.r.style_of(window, "transform").as_deref(),
        Some(format!("translateY({}px)", (100 - OVERSCAN) * 40).as_str())
    );
    // a small scroll keeps the rows it keeps and touches two
    let before = s.r.live_nodes();
    s.r.clear_ops();
    s.r.fire(
        s.list,
        &Event::Scroll(ScrollEvent {
            top: 4040.0,
            left: 0.0,
        }),
    );
    let creates =
        s.r.ops()
            .iter()
            .filter(|o| matches!(o, quench_ui::Op::CreateElement(_)))
            .count();
    assert_eq!(
        creates, 2,
        "one row (a wrapper and its span) came into view"
    );
    assert!(s.r.live_nodes().abs_diff(before) <= 4, "and one went out");
    // to the very end
    s.r.fire(
        s.list,
        &Event::Scroll(ScrollEvent {
            top: 399_360.0,
            left: 0.0,
        }),
    );
    assert_eq!(rows(&s).last().unwrap(), "row 9999");
    // an overshoot (elastic scrolling) shows the top
    s.r.fire(
        s.list,
        &Event::Scroll(ScrollEvent {
            top: -80.0,
            left: 0.0,
        }),
    );
    assert_eq!(first_row(&s), 0);
}

#[quench_test::test]
fn the_real_viewport_height_replaces_the_assumed_one() {
    let s = setup(1000);
    s.r.fire(
        s.list,
        &Event::Resize(Size {
            width: 300.0,
            height: 200.0,
        }),
    );
    assert_eq!(rows(&s).len(), 5 + 1 + OVERSCAN);
    s.r.fire(
        s.list,
        &Event::Resize(Size {
            width: 300.0,
            height: 2000.0,
        }),
    );
    assert_eq!(rows(&s).len(), 50 + 1 + OVERSCAN);
}

#[quench_test::test]
fn the_list_follows_changes_to_its_items() {
    let s = setup(3);
    assert_eq!(rows(&s), ["row 0", "row 1", "row 2"]);
    s.items.update(|v| v.insert(1, 99));
    assert_eq!(rows(&s), ["row 0", "row 99", "row 1", "row 2"]);
    let spacer = s.r.children(s.list)[0];
    assert_eq!(s.r.style_of(spacer, "height").as_deref(), Some("160px"));
    s.items.set(Vec::new());
    assert!(rows(&s).is_empty());
    assert_eq!(s.r.style_of(spacer, "height").as_deref(), Some("0px"));
    // a scroll position beyond a list that got shorter shows what is there
    s.items.set((0..50).collect());
    s.r.fire(
        s.list,
        &Event::Scroll(ScrollEvent {
            top: 800.0,
            left: 0.0,
        }),
    );
    s.items.set((0..10).collect());
    assert!(rows(&s).len() <= 10);
}
