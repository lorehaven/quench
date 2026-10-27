//! Gesture recognition, with a clock the test moves.

use quench_ui::gesture::{Axis, Config, Direction, Gesture, Recognizer, on_gesture};
use quench_ui::platform::{self, ManualPlatform};
use quench_ui::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

fn run(r: &mut Recognizer, steps: &[(&str, f64, f64, f64)]) -> Vec<Gesture> {
    let mut out = Vec::new();
    for (what, x, y, t) in steps {
        match *what {
            "down" => r.down(*x, *y, *t),
            "move" => out.extend(r.moved(*x, *y, *t)),
            "up" => out.extend(r.up(*x, *y, *t)),
            "long" => out.extend(r.long_press_due(*t)),
            _ => out.extend(r.cancel()),
        }
    }
    out
}

#[quench_test::test]
fn a_press_released_in_place_is_a_tap_and_small_wobble_is_ignored() {
    let mut r = Recognizer::new(Config::default());
    let out = run(
        &mut r,
        &[
            ("down", 5.0, 5.0, 0.0),
            ("move", 9.0, 8.0, 20.0),
            ("up", 9.0, 8.0, 80.0),
        ],
    );
    assert_eq!(out, [Gesture::Tap]);
    assert!(r.up(0.0, 0.0, 0.0).is_empty(), "nothing is down");
    assert!(r.moved(0.0, 0.0, 0.0).is_empty());
    assert!(r.cancel().is_empty());
    assert_eq!(r.long_press_due(10_000.0), None);
}

#[quench_test::test]
fn holding_still_is_a_long_press_and_the_release_is_not_also_a_tap() {
    let mut r = Recognizer::new(Config::default());
    let out = run(
        &mut r,
        &[
            ("down", 0.0, 0.0, 0.0),
            ("long", 0.0, 0.0, 300.0),
            ("long", 0.0, 0.0, 500.0),
            ("long", 0.0, 0.0, 600.0),
            ("up", 0.0, 0.0, 700.0),
        ],
    );
    assert_eq!(out, [Gesture::LongPress], "once, and not before the time");
}

#[quench_test::test]
fn moving_past_the_slop_starts_a_drag_that_reports_offsets_and_ends_with_a_velocity() {
    let mut r = Recognizer::new(Config::default());
    let out = run(
        &mut r,
        &[
            ("down", 0.0, 0.0, 0.0),
            ("move", 30.0, 0.0, 100.0),
            ("move", 55.0, 5.0, 200.0),
            ("up", 55.0, 5.0, 200.0),
        ],
    );
    assert_eq!(out[0], Gesture::DragStart);
    assert_eq!(out[1], Gesture::Drag { dx: 30.0, dy: 0.0 });
    assert_eq!(out[2], Gesture::Drag { dx: 55.0, dy: 5.0 });
    let Gesture::DragEnd { dx, dy, vx, .. } = out[3] else {
        panic!("{out:?}")
    };
    assert_eq!((dx, dy), (55.0, 5.0));
    assert!((vx - 0.25).abs() < 1e-9, "25 px in the last 100 ms: {vx}");
    assert_eq!(
        out.len(),
        4,
        "0.25 px/ms is under the swipe speed: a drag, not a swipe"
    );
}

#[quench_test::test]
fn a_fast_far_drag_is_a_swipe_in_its_dominant_direction() {
    for (to, direction) in [
        ((120.0, 10.0), Direction::Right),
        ((-120.0, 10.0), Direction::Left),
        ((10.0, 120.0), Direction::Down),
        ((10.0, -120.0), Direction::Up),
    ] {
        let mut r = Recognizer::new(Config::default());
        let out = run(
            &mut r,
            &[
                ("down", 0.0, 0.0, 0.0),
                ("move", to.0 / 2.0, to.1 / 2.0, 50.0),
                ("move", to.0, to.1, 100.0),
                ("up", to.0, to.1, 100.0),
            ],
        );
        assert_eq!(out.last(), Some(&Gesture::Swipe(direction)), "{to:?}");
    }
    let mut r = Recognizer::new(Config::default());
    let slow = run(
        &mut r,
        &[
            ("down", 0.0, 0.0, 0.0),
            ("move", 60.0, 0.0, 1000.0),
            ("move", 120.0, 0.0, 2000.0),
            ("up", 120.0, 0.0, 2000.0),
        ],
    );
    assert!(
        !slow.iter().any(|g| matches!(g, Gesture::Swipe(_))),
        "slow is a drag, not a swipe"
    );
    let mut r = Recognizer::new(Config::default());
    let short = run(
        &mut r,
        &[
            ("down", 0.0, 0.0, 0.0),
            ("move", 20.0, 0.0, 10.0),
            ("up", 20.0, 0.0, 20.0),
        ],
    );
    assert!(
        !short.iter().any(|g| matches!(g, Gesture::Swipe(_))),
        "short is a drag, not a swipe"
    );
    // two moves at the same instant do not divide by zero
    let mut r = Recognizer::new(Config::default());
    let out = run(
        &mut r,
        &[
            ("down", 0.0, 0.0, 0.0),
            ("move", 50.0, 0.0, 10.0),
            ("move", 90.0, 0.0, 10.0),
            ("up", 90.0, 0.0, 10.0),
        ],
    );
    assert!(
        matches!(out.last(), Some(Gesture::Swipe(Direction::Right))),
        "{out:?}"
    );
}

#[quench_test::test]
fn cancelling_a_drag_says_so() {
    let mut r = Recognizer::new(Config::default());
    let out = run(
        &mut r,
        &[
            ("down", 0.0, 0.0, 0.0),
            ("move", 40.0, 0.0, 10.0),
            ("cancel", 0.0, 0.0, 0.0),
        ],
    );
    assert_eq!(out.last(), Some(&Gesture::Cancel));
}

fn pointer(x: f64, y: f64) -> PointerEvent {
    PointerEvent {
        x,
        y,
        pointer_id: 1,
    }
}

#[quench_test::test]
fn an_element_reports_gestures_from_its_pointer_events_and_times_the_long_press() {
    let clock = Rc::new(ManualPlatform::new());
    platform::install(clock.clone());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let r = Rc::new(TestRenderer::new());
    let el = on_gesture(
        div(),
        Config {
            axis: Axis::Horizontal,
            ..Config::default()
        },
        move |g| log.borrow_mut().push(*g),
    );
    let m = mount(&r, &r.root(), el);
    let node = r.children(r.root())[0];
    assert_eq!(r.attr(node, "class").as_deref(), Some("q-touch-pan-y"));

    r.fire(node, &Event::PointerDown(pointer(0.0, 0.0)));
    clock.advance(100.0);
    r.fire(node, &Event::PointerUp(pointer(0.0, 0.0)));
    assert_eq!(*seen.borrow(), [Gesture::Tap]);
    assert_eq!(
        clock.pending(),
        0,
        "the long press timer was cancelled by the release"
    );

    seen.borrow_mut().clear();
    r.fire(node, &Event::PointerDown(pointer(0.0, 0.0)));
    clock.advance(500.0);
    assert_eq!(*seen.borrow(), [Gesture::LongPress]);
    r.fire(node, &Event::PointerUp(pointer(0.0, 0.0)));
    assert_eq!(seen.borrow().len(), 1);

    seen.borrow_mut().clear();
    r.fire(node, &Event::PointerDown(pointer(0.0, 0.0)));
    clock.advance(50.0);
    r.fire(node, &Event::PointerMove(pointer(100.0, 0.0)));
    assert_eq!(clock.pending(), 0, "moving away cancels the long press");
    r.fire(node, &Event::PointerCancel(pointer(100.0, 0.0)));
    assert_eq!(seen.borrow().last(), Some(&Gesture::Cancel));
    m.unmount();

    for (axis, class) in [
        (Axis::Both, "q-touch-none"),
        (Axis::Vertical, "q-touch-pan-x"),
    ] {
        let r = Rc::new(TestRenderer::new());
        let m = mount(
            &r,
            &r.root(),
            on_gesture(
                div(),
                Config {
                    axis,
                    ..Config::default()
                },
                |_| {},
            ),
        );
        assert_eq!(
            r.attr(r.children(r.root())[0], "class").as_deref(),
            Some(class)
        );
        m.unmount();
    }
}
