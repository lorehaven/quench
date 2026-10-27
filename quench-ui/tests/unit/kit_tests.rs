//! The component kit: structure, ARIA, keyboard, and what it does with taps and drags.

use quench_ui::form::{Field, rules};
use quench_ui::gesture::Gesture;
use quench_ui::kit::{
    NavItem, PULL_THRESHOLD, SWIPE_REVEAL, app_bar, bottom_nav, checkbox, chip, date_field, dialog,
    fab, icon_button, list, list_tile, pull_to_refresh, sheet, snackbar, swipe_action, switch,
    tabs, text_field, time_field,
};
use quench_ui::platform::{self, ManualPlatform};
use quench_ui::prelude::*;
use quench_ui::{ScrollEvent, take_prevent_request};
use std::cell::Cell;
use std::rc::Rc;

fn setup() -> (Rc<TestRenderer>, Rc<ManualPlatform>) {
    let clock = Rc::new(ManualPlatform::new());
    platform::install(clock.clone());
    (Rc::new(TestRenderer::new()), clock)
}

fn key(name: &str) -> Event {
    Event::KeyDown(KeyEvent {
        key: name.into(),
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    })
}

fn p(x: f64, y: f64) -> PointerEvent {
    PointerEvent {
        x,
        y,
        pointer_id: 1,
    }
}

#[quench_test::test]
fn the_app_bar_has_a_title_and_places_its_controls() {
    let (r, _) = setup();
    let clicked = Rc::new(Cell::new(false));
    let c = clicked.clone();
    let bar = app_bar(
        "Notes",
        Some(icon_button("Back", "\u{2190}", move || c.set(true)).into_view()),
        vec![icon_button("Search", "?", || {}).into_view()],
    );
    let m = mount(&r, &r.root(), bar);
    assert_eq!(r.text_of(r.find_by_tag("h1")[0]), "Notes");
    let buttons = r.find_by_tag("button");
    assert_eq!(r.attr(buttons[0], "aria-label").as_deref(), Some("Back"));
    assert_eq!(
        r.attr(r.children(buttons[0])[0], "aria-hidden").as_deref(),
        Some("true"),
        "the glyph is decoration"
    );
    r.fire(buttons[0], &Event::Click);
    assert!(clicked.get());
    assert_eq!(r.find_by_tag("header").len(), 1);
    m.unmount();
}

#[quench_test::test]
fn bottom_navigation_marks_and_changes_the_current_destination() {
    let (r, _) = setup();
    let selected = create_signal(0);
    let m = mount(
        &r,
        &r.root(),
        bottom_nav(
            "Main",
            vec![NavItem::new("Home", "H"), NavItem::new("Search", "S")],
            &selected,
        ),
    );
    let nav = r.find_by_tag("nav")[0];
    assert_eq!(r.attr(nav, "aria-label").as_deref(), Some("Main"));
    let items = r.find_by_tag("button");
    assert_eq!(r.attr(items[0], "aria-current").as_deref(), Some("page"));
    assert_eq!(r.attr(items[1], "aria-current"), None);
    r.fire(items[1], &Event::Click);
    assert_eq!(selected.get_untracked(), 1);
    assert_eq!(r.attr(items[0], "aria-current"), None);
    assert_eq!(r.attr(items[1], "aria-current").as_deref(), Some("page"));
    m.unmount();
}

#[quench_test::test]
fn a_sheet_is_a_modal_dialog_that_closes_on_escape_and_on_the_scrim() {
    let (r, clock) = setup();
    let open = create_signal(false);
    let m = mount(&r, &r.root(), sheet(&open, "Filters", || p_text("inside")));
    assert_eq!(r.text(), "");
    open.set(true);
    let dialog = r.find_by_attr("role", "dialog")[0];
    assert_eq!(r.attr(dialog, "aria-modal").as_deref(), Some("true"));
    assert_eq!(r.attr(dialog, "aria-label").as_deref(), Some("Filters"));
    assert_eq!(r.text_of(r.find_by_tag("h1")[0]), "Filters");
    assert!(r.text().contains("inside"));
    // it takes focus and traps it
    assert!(r.ops().iter().any(
        |o| matches!(o, quench_ui::Op::Listen(n, quench_ui::EventKind::KeyDown) if *n == dialog)
    ));
    r.fire(dialog, &key("a"));
    assert!(open.get_untracked(), "other keys do nothing");
    r.fire(dialog, &key("Escape"));
    assert!(!open.get_untracked());
    clock.advance(1000.0);
    assert_eq!(r.text(), "");
    open.set(true);
    let scrim = r.find_by_attr("class", "q-scrim")[0];
    r.fire(scrim, &Event::Click);
    assert!(!open.get_untracked());
    m.unmount();
}

fn p_text(text: &str) -> View {
    quench_ui::view::p().text(text).into_view()
}

#[quench_test::test]
fn a_dialog_shows_its_content_and_actions() {
    let (r, clock) = setup();
    let open = create_signal(true);
    let closer = open.clone();
    let m = mount(
        &r,
        &r.root(),
        dialog(
            &open,
            "Delete?",
            || p_text("This cannot be undone."),
            move || {
                let closer = closer.clone();
                vec![
                    button()
                        .text("Cancel")
                        .on_click(move || closer.set(false))
                        .into_view(),
                ]
            },
        ),
    );
    assert!(r.text().contains("This cannot be undone."));
    let cancel = r.find_by_tag("button")[0];
    r.fire(cancel, &Event::Click);
    clock.advance(1000.0);
    assert_eq!(r.text(), "");
    m.unmount();
}

#[quench_test::test]
fn a_snackbar_announces_a_message_and_keeps_its_text_while_it_leaves() {
    let (r, clock) = setup();
    let message = create_signal(None);
    let undone = Rc::new(Cell::new(0));
    let u = undone.clone();
    let m = mount(
        &r,
        &r.root(),
        snackbar(
            &message,
            Some(("Undo", Rc::new(move || u.set(u.get() + 1)))),
        ),
    );
    assert_eq!(r.text(), "");
    message.set(Some("Note deleted".into()));
    let bar = r.find_by_attr("role", "status")[0];
    assert_eq!(r.attr(bar, "aria-live").as_deref(), Some("polite"));
    assert!(r.text().contains("Note deleted"));
    message.set(None);
    assert!(
        r.text().contains("Note deleted"),
        "still readable while it fades out"
    );
    clock.advance(1000.0);
    assert_eq!(r.text(), "");
    message.set(Some("Again".into()));
    let action = r.find_by_tag("button")[0];
    r.fire(action, &Event::Click);
    assert_eq!(undone.get(), 1);
    assert_eq!(
        message.get_untracked(),
        None,
        "using the action dismisses it"
    );
    let plain = Rc::new(TestRenderer::new());
    let both = create_signal(Some("x".to_string()));
    let m2 = mount(&plain, &plain.root(), snackbar(&both, None));
    assert!(plain.find_by_tag("button").is_empty());
    m2.unmount();
    m.unmount();
}

#[quench_test::test]
fn list_tiles_are_rows_and_tappable_ones_are_buttons() {
    let (r, _) = setup();
    let tapped = Rc::new(Cell::new(0));
    let t = tapped.clone();
    let view = list(vec![
        list_tile("Plain", Some("sub"), None, None, None),
        list_tile(
            "Tap me",
            None,
            Some(span().text("L").into_view()),
            Some(span().text("R").into_view()),
            Some(Rc::new(move || t.set(t.get() + 1))),
        ),
    ]);
    let m = mount(&r, &r.root(), view);
    assert_eq!(r.find_by_tag("li").len(), 2);
    assert_eq!(r.find_by_tag("button").len(), 1);
    r.fire(r.find_by_tag("button")[0], &Event::Click);
    assert_eq!(tapped.get(), 1);
    assert!(
        r.text().contains("sub") && r.text().contains("LTap meR"),
        "{}",
        r.text()
    );
    m.unmount();
}

#[quench_test::test]
fn the_switch_checkbox_and_chip_toggle_and_say_so() {
    let (r, _) = setup();
    let (on, ticked, pressed) = (
        create_signal(false),
        create_signal(true),
        create_signal(false),
    );
    let m = mount(
        &r,
        &r.root(),
        div()
            .child(switch("Wi-Fi", &on))
            .child(checkbox("Agree", &ticked))
            .child(chip("Open", &pressed)),
    );
    let sw = r.find_by_attr("role", "switch")[0];
    assert_eq!(r.attr(sw, "aria-checked").as_deref(), Some("false"));
    r.fire(sw, &Event::Click);
    assert!(on.get_untracked());
    assert_eq!(r.attr(sw, "aria-checked").as_deref(), Some("true"));

    let cb = r.find_by_attr("type", "checkbox")[0];
    assert_eq!(r.attr(cb, "checked").as_deref(), Some(""));
    r.fire(cb, &Event::Click);
    assert!(!ticked.get_untracked());
    assert_eq!(r.attr(cb, "checked"), None);

    let chip_node = r.find_by_attr("aria-pressed", "false")[0];
    r.fire(chip_node, &Event::Click);
    assert!(pressed.get_untracked());
    assert_eq!(r.attr(chip_node, "aria-pressed").as_deref(), Some("true"));
    assert!(r.attr(chip_node, "class").unwrap().contains("q-chip-on"));
    m.unmount();
}

#[quench_test::test]
fn a_text_field_is_labelled_hinted_and_shows_its_error() {
    let (r, _) = setup();
    let email = Field::new("email", "").rule(rules::required("Enter your email"));
    let m = mount(
        &r,
        &r.root(),
        text_field("Email", &email, "email", Some("name@example.com")),
    );
    let label = r.find_by_tag("label")[0];
    assert_eq!(r.attr(label, "for").as_deref(), Some("email"));
    let input = r.find_by_tag("input")[0];
    assert_eq!(r.attr(input, "type").as_deref(), Some("email"));
    assert_eq!(
        r.attr(input, "placeholder").as_deref(),
        Some("name@example.com")
    );
    r.fire(input, &Event::Blur);
    assert!(r.text().contains("Enter your email"));
    assert_eq!(r.attr(input, "aria-invalid").as_deref(), Some("true"));
    m.unmount();

    let date = Field::new("when", "2024-02-30").rule(rules::date("Not a date"));
    let time = Field::new("at", "24:00").rule(rules::time("Not a time"));
    assert!(!date.validate() && !time.validate());
    for (v, ok) in [
        ("2024-02-29", true),
        ("2023-02-29", false),
        ("2000-02-29", true),
        ("1900-02-29", false),
        ("2024-04-31", false),
        ("2024-13-01", false),
        ("2024-1-01", false),
        ("not", false),
        ("2024-12-31", true),
    ] {
        date.set(v);
        assert_eq!(date.validate(), ok, "{v}");
    }
    for (v, ok) in [
        ("00:00", true),
        ("23:59", true),
        ("24:00", false),
        ("12:60", false),
        ("1:30", false),
        ("noon", false),
    ] {
        time.set(v);
        assert_eq!(time.validate(), ok, "{v}");
    }
    let m = mount(
        &r,
        &r.root(),
        div()
            .child(date_field("Date", &date))
            .child(time_field("Time", &time)),
    );
    assert_eq!(
        r.find_by_attr("type", "date").len() + r.find_by_attr("type", "time").len(),
        2
    );
    m.unmount();
}

#[quench_test::test]
fn the_fab_has_an_accessible_name() {
    let (r, _) = setup();
    let m = mount(&r, &r.root(), fab("New note", "+", || {}));
    let b = r.find_by_tag("button")[0];
    assert_eq!(r.attr(b, "aria-label").as_deref(), Some("New note"));
    assert!(r.attr(b, "class").unwrap().contains("q-fab"));
    m.unmount();
}

#[quench_test::test]
fn tabs_follow_the_keyboard_pattern() {
    let (r, _) = setup();
    let selected = create_signal(0);
    let m = mount(
        &r,
        &r.root(),
        tabs("t", &["One", "Two", "Three"], &selected, |i| {
            p_text(&format!("panel {i}"))
        }),
    );
    let tab_nodes = r.find_by_attr("role", "tab");
    assert_eq!(tab_nodes.len(), 3);
    assert_eq!(
        r.attr(r.find_by_attr("role", "tablist")[0], "role")
            .as_deref(),
        Some("tablist")
    );
    let panel = r.find_by_attr("role", "tabpanel")[0];
    assert_eq!(r.text_of(panel), "panel 0");
    assert_eq!(r.attr(panel, "aria-labelledby").as_deref(), Some("t-tab-0"));
    assert_eq!(
        r.attr(tab_nodes[0], "aria-selected").as_deref(),
        Some("true")
    );
    assert_eq!(r.attr(tab_nodes[0], "tabindex").as_deref(), Some("0"));
    assert_eq!(r.attr(tab_nodes[1], "tabindex").as_deref(), Some("-1"));
    assert_eq!(
        r.attr(tab_nodes[1], "aria-controls").as_deref(),
        Some("t-panel")
    );

    let _ = take_prevent_request();
    r.fire(tab_nodes[0], &key("ArrowRight"));
    assert!(take_prevent_request(), "the arrow key is ours");
    assert_eq!(selected.get_untracked(), 1);
    assert_eq!(r.text_of(panel), "panel 1");
    assert_eq!(
        r.attr(tab_nodes[1], "aria-selected").as_deref(),
        Some("true")
    );
    assert!(r.ops().iter().any(|o| matches!(o, quench_ui::Op::SetAttr(n, name, v) if *n == tab_nodes[1] && name == "tabindex" && v == "0")));
    for (k, want) in [
        ("ArrowRight", 2),
        ("ArrowRight", 0),
        ("ArrowLeft", 2),
        ("Home", 0),
        ("End", 2),
    ] {
        r.fire(tab_nodes[selected.get_untracked()], &key(k));
        assert_eq!(selected.get_untracked(), want, "{k}");
    }
    let _ = take_prevent_request();
    r.fire(tab_nodes[0], &key("Tab"));
    assert!(!take_prevent_request(), "Tab keeps moving through the page");
    r.fire(tab_nodes[1], &Event::Click);
    assert_eq!(selected.get_untracked(), 1);
    m.unmount();
}

#[quench_test::test]
fn pulling_down_at_the_top_refreshes() {
    let (r, _) = setup();
    let refreshing = create_signal(false);
    let calls = Rc::new(Cell::new(0));
    let c = calls.clone();
    let m = mount(
        &r,
        &r.root(),
        pull_to_refresh(&refreshing, move || c.set(c.get() + 1), p_text("content")),
    );
    let area = r.children(r.root())[0];
    assert!(r.attr(area, "class").unwrap().contains("q-ptr-top"));
    let drag = |dy: f64| {
        r.fire(area, &Event::PointerDown(p(0.0, 0.0)));
        r.fire(area, &Event::PointerMove(p(0.0, dy / 2.0)));
        r.fire(area, &Event::PointerMove(p(0.0, dy)));
        r.fire(area, &Event::PointerUp(p(0.0, dy)));
    };
    drag(PULL_THRESHOLD / 2.0);
    assert_eq!(
        (calls.get(), refreshing.get_untracked()),
        (0, false),
        "too short"
    );
    drag(PULL_THRESHOLD + 20.0);
    assert_eq!((calls.get(), refreshing.get_untracked()), (1, true));
    drag(PULL_THRESHOLD + 20.0);
    assert_eq!(calls.get(), 1, "not again while it is busy");
    refreshing.set(false);
    // scrolled down: a pull is the page's, not ours
    r.fire(
        area,
        &Event::Scroll(ScrollEvent {
            top: 300.0,
            left: 0.0,
        }),
    );
    assert!(!r.attr(area, "class").unwrap().contains("q-ptr-top"));
    drag(PULL_THRESHOLD + 20.0);
    assert_eq!(calls.get(), 1);
    r.fire(
        area,
        &Event::Scroll(ScrollEvent {
            top: 0.0,
            left: 0.0,
        }),
    );
    r.fire(area, &Event::PointerDown(p(0.0, 0.0)));
    r.fire(area, &Event::PointerMove(p(0.0, 90.0)));
    r.fire(area, &Event::PointerCancel(p(0.0, 90.0)));
    assert_eq!(calls.get(), 1, "a cancelled pull refreshes nothing");
    m.unmount();
}

#[quench_test::test]
fn swiping_a_row_reveals_its_action() {
    let (r, _) = setup();
    let done = Rc::new(Cell::new(0));
    let d = done.clone();
    let m = mount(
        &r,
        &r.root(),
        swipe_action(p_text("row"), "Delete", move || d.set(d.get() + 1)),
    );
    let row = r.children(r.root())[0];
    let front = r.find_by_attr("class", "q-swipe-front")[0];
    let slide = |dx: f64| {
        r.fire(row, &Event::PointerDown(p(200.0, 0.0)));
        r.fire(row, &Event::PointerMove(p(200.0 + dx / 2.0, 0.0)));
        r.fire(row, &Event::PointerMove(p(200.0 + dx, 0.0)));
        r.fire(row, &Event::PointerUp(p(200.0 + dx, 0.0)));
    };
    slide(-20.0);
    assert_eq!(
        r.style_of(front, "transform").as_deref(),
        Some("translateX(0px)"),
        "a short drag springs back"
    );
    slide(-70.0);
    assert_eq!(
        r.style_of(front, "transform").as_deref(),
        Some(format!("translateX({}px)", -SWIPE_REVEAL).as_str())
    );
    slide(-200.0);
    assert_eq!(
        r.style_of(front, "transform").as_deref(),
        Some(format!("translateX({}px)", -SWIPE_REVEAL).as_str()),
        "no further than the action"
    );
    slide(100.0);
    assert_eq!(
        r.style_of(front, "transform").as_deref(),
        Some("translateX(0px)"),
        "dragged back closed"
    );
    slide(-70.0);
    r.fire(row, &Event::PointerDown(p(200.0, 0.0)));
    r.fire(row, &Event::PointerMove(p(150.0, 0.0)));
    r.fire(row, &Event::PointerCancel(p(150.0, 0.0)));
    assert_eq!(
        r.style_of(front, "transform").as_deref(),
        Some("translateX(0px)")
    );
    let action = r.find_by_tag("button")[0];
    r.fire(action, &Event::Click);
    assert_eq!(done.get(), 1);
    let _ = Gesture::Tap;
    m.unmount();
}
