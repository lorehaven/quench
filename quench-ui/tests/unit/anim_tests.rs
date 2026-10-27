//! Presence: content that stays until its exit has played.

use quench_ui::anim::presence;
use quench_ui::platform::{self, ManualPlatform};
use quench_ui::prelude::*;
use std::rc::Rc;

fn setup(
    initial: bool,
) -> (
    Rc<TestRenderer>,
    Rc<ManualPlatform>,
    Signal<bool>,
    Mount<TestRenderer>,
) {
    let clock = Rc::new(ManualPlatform::new());
    platform::install(clock.clone());
    let r = Rc::new(TestRenderer::new());
    let visible = create_signal(initial);
    let v = visible.clone();
    let m = mount(
        &r,
        &r.root(),
        presence(move || v.get(), 200, || p().text("hello").into_view()),
    );
    (r, clock, visible, m)
}

fn classes(r: &TestRenderer) -> Option<String> {
    r.find_by_tag("div")
        .first()
        .and_then(|n| r.attr(*n, "class"))
}

#[quench_test::test]
fn content_enters_in_two_steps_and_leaves_after_its_transition() {
    let (r, clock, visible, m) = setup(false);
    assert_eq!(r.text(), "");
    visible.set(true);
    assert_eq!(
        classes(&r).as_deref(),
        Some("q-anim"),
        "first drawn in the entering state"
    );
    let node = r.find_by_tag("p")[0];
    clock.advance(20.0);
    assert_eq!(classes(&r).as_deref(), Some("q-anim q-anim-in"));
    assert_eq!(r.find_by_tag("p")[0], node, "the same content, not rebuilt");

    visible.set(false);
    assert_eq!(
        classes(&r).as_deref(),
        Some("q-anim"),
        "the exit state, content still there"
    );
    assert_eq!(r.text(), "hello");
    let wrapper = r.find_by_tag("div")[0];
    r.fire(wrapper, &Event::TransitionEnd);
    assert_eq!(
        r.text(),
        "",
        "gone when the browser says the transition ended"
    );
    assert_eq!(clock.pending(), 1, "the fallback timer is still waiting");
    clock.advance(1000.0);
    assert_eq!(clock.pending(), 0);
    m.unmount();
}

#[quench_test::test]
fn the_fallback_removes_content_when_transitionend_never_comes() {
    let (r, clock, visible, m) = setup(true);
    assert_eq!(
        classes(&r).as_deref(),
        Some("q-anim q-anim-in"),
        "shown at once, nothing to animate"
    );
    visible.set(false);
    clock.advance(200.0);
    assert_eq!(r.text(), "hello");
    clock.advance(60.0);
    assert_eq!(r.text(), "");
    m.unmount();
}

#[quench_test::test]
fn showing_again_during_the_exit_keeps_the_content() {
    let (r, clock, visible, m) = setup(true);
    let node = r.find_by_tag("p")[0];
    visible.set(false);
    clock.advance(100.0);
    visible.set(true);
    assert_eq!(classes(&r).as_deref(), Some("q-anim q-anim-in"));
    clock.advance(1000.0);
    assert_eq!(r.find_by_tag("p")[0], node);
    // hiding during the entrance
    visible.set(false);
    clock.advance(1000.0);
    visible.set(true);
    visible.set(false);
    clock.advance(1000.0);
    assert_eq!(r.text(), "");
    m.unmount();
}

#[quench_test::test]
fn reduced_motion_skips_the_waiting() {
    let (r, clock, visible, m) = setup(false);
    clock.set_reduced_motion(true);
    visible.set(true);
    assert_eq!(classes(&r).as_deref(), Some("q-anim q-anim-in"));
    visible.set(false);
    assert_eq!(r.text(), "");
    assert_eq!(clock.pending(), 0);
    m.unmount();
}

#[quench_test::test]
fn unmounting_cancels_the_timers() {
    let (r, clock, visible, m) = setup(true);
    visible.set(false);
    assert_eq!(clock.pending(), 1);
    m.unmount();
    assert_eq!(clock.pending(), 0);
    drop(r);
}
