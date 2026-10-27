//! The router over the browser's history, and the hints the router needs from the host.

use quench_ui::prelude::*;
use quench_ui::router::{Router, Routes, link};
use quench_ui::{ScrollEvent, Size};
use quench_ui_web::mock::{self, Call};
use quench_ui_web::{BrowserHistory, DomRenderer, decode_event, kind_code, mount_root};
use std::rc::Rc;

fn site() -> View {
    let router = Router::new(Rc::new(BrowserHistory::new()));
    Routes::new()
        .route("/", |_| div().child(link("/next").text("next")).into_view())
        .route("/next", |_| p().text("next page").into_view())
        .view(&router)
}

#[quench_test::test]
fn the_router_starts_at_the_pages_url_and_pushes_entries() {
    mock::reset();
    mock::set_location("/?from=a#top");
    let m = mount_root(site());
    assert_eq!(
        mock::html().replace("<!---->", ""),
        "<div><a href=\"/next\" aria-current=\"page\">next</a></div>"
            .replace(" aria-current=\"page\"", "")
    );
    let a = mock::handles_by_tag("a")[0];
    let calls = mock::calls();
    assert!(
        calls.contains(&Call::Prevent(a, kind_code(EventKind::Click), 2)),
        "a link asks for plain clicks only: {calls:?}"
    );
    assert!(mock::fire_click(a));
    assert_eq!(mock::text(), "next page");
    assert!(mock::calls().contains(&Call::History(0, "/next".into())));
    assert_eq!(mock::history_entries().1, 1);
    m.unmount();
}

#[quench_test::test]
fn the_back_button_shows_the_previous_page() {
    mock::reset();
    let m = mount_root(site());
    mock::fire_click(mock::handles_by_tag("a")[0]);
    assert_eq!(mock::text(), "next page");
    // the browser's own back button: it reports the URL it landed on
    mock::popstate("/");
    assert_eq!(mock::text(), "next");
    m.unmount();
}

#[quench_test::test]
fn router_back_and_forward_go_through_the_browser() {
    mock::reset();
    let router = Router::new(Rc::new(BrowserHistory::new()));
    let r2 = router.clone();
    let m = mount_root(
        Routes::new()
            .route("/", |_| p().text("one").into_view())
            .route("/two", |_| p().text("two").into_view())
            .view(&router),
    );
    r2.navigate("/two");
    assert_eq!(mock::text(), "two");
    assert!(r2.back());
    assert_eq!(mock::text(), "one");
    assert!(r2.forward());
    assert_eq!(mock::text(), "two");
    r2.replace("/");
    assert_eq!(mock::text(), "one");
    assert!(mock::calls().contains(&Call::History(1, "/".into())));
    m.unmount();
}

#[quench_test::test]
fn focus_and_default_actions_reach_the_host() {
    mock::reset();
    let m = mount_root(
        div()
            .child(
                input()
                    .focus_on_mount()
                    .prevent_default(EventKind::KeyDown)
                    .on_keydown(|_| {}),
            )
            .child(div().trap_focus()),
    );
    let calls = mock::calls();
    let input = mock::handles_by_tag("input")[0];
    assert!(calls.contains(&Call::Focus(input)));
    assert!(calls.contains(&Call::Prevent(input, kind_code(EventKind::KeyDown), 1)));
    let dialog = mock::handles_by_tag("div")[1];
    assert!(calls.contains(&Call::TrapFocus(dialog)));
    assert!(calls.contains(&Call::Focus(dialog)));
    m.unmount();
}

#[quench_test::test]
fn the_new_event_kinds_decode() {
    let get = |name: &str, payload: &str, a: f64, b: f64| {
        let kind = [
            EventKind::CompositionStart,
            EventKind::CompositionEnd,
            EventKind::Scroll,
            EventKind::TransitionEnd,
            EventKind::Resize,
        ]
        .into_iter()
        .find(|k| k.dom_name() == name)
        .unwrap();
        decode_event(kind_code(kind), payload.into(), a, b, 0, 0).unwrap()
    };
    assert_eq!(
        get("compositionstart", "n", 0.0, 0.0),
        Event::CompositionStart("n".into())
    );
    assert_eq!(
        get("compositionend", "ń", 0.0, 0.0),
        Event::CompositionEnd("ń".into())
    );
    assert_eq!(
        get("scroll", "", 40.0, 2.0),
        Event::Scroll(ScrollEvent {
            top: 40.0,
            left: 2.0
        })
    );
    assert_eq!(get("transitionend", "", 0.0, 0.0), Event::TransitionEnd);
    assert_eq!(
        get("resize", "", 320.0, 200.0),
        Event::Resize(Size {
            width: 320.0,
            height: 200.0
        })
    );
    let _ = DomRenderer;
}
