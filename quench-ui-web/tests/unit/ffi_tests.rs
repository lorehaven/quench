//! The exported functions: buffers, event decoding, and what they do with bad input.

use quench_ui::{Event, EventKind, KeyEvent, PointerEvent};
use quench_ui_web::mock;
use quench_ui_web::{decode_event, kind_code, q_alloc, q_dealloc, q_dispatch_event, q_fetch_done};
use std::cell::RefCell;
use std::rc::Rc;

#[quench_test::test]
fn kinds_round_trip_through_their_numbers() {
    let kinds = [
        EventKind::Click,
        EventKind::DoubleClick,
        EventKind::Input,
        EventKind::Change,
        EventKind::KeyDown,
        EventKind::KeyUp,
        EventKind::Submit,
        EventKind::Focus,
        EventKind::Blur,
        EventKind::PointerDown,
        EventKind::PointerUp,
        EventKind::PointerMove,
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        assert_eq!(kind_code(kind), u32::try_from(i).unwrap());
        let event = decode_event(kind_code(kind), "x".into(), 1.0, 2.0, 3, 0).unwrap();
        assert_eq!(event.kind(), kind);
    }
}

#[quench_test::test]
fn decoding_fills_in_payloads_and_modifier_flags() {
    assert_eq!(
        decode_event(2, "hi".into(), 0.0, 0.0, 0, 0),
        Some(Event::Input("hi".into()))
    );
    assert_eq!(
        decode_event(3, "v".into(), 0.0, 0.0, 0, 0),
        Some(Event::Change("v".into()))
    );
    assert_eq!(
        decode_event(4, "a".into(), 0.0, 0.0, 0, 0b1111),
        Some(Event::KeyDown(KeyEvent {
            key: "a".into(),
            ctrl: true,
            shift: true,
            alt: true,
            meta: true
        }))
    );
    assert_eq!(
        decode_event(5, "b".into(), 0.0, 0.0, 0, 0b0101),
        Some(Event::KeyUp(KeyEvent {
            key: "b".into(),
            ctrl: true,
            shift: false,
            alt: true,
            meta: false
        }))
    );
    assert_eq!(
        decode_event(9, String::new(), 10.5, 20.5, 7, 0),
        Some(Event::PointerDown(PointerEvent {
            x: 10.5,
            y: 20.5,
            pointer_id: 7
        }))
    );
    assert_eq!(
        decode_event(0, String::new(), 0.0, 0.0, 0, 0),
        Some(Event::Click)
    );
    assert_eq!(
        decode_event(6, String::new(), 0.0, 0.0, 0, 0),
        Some(Event::Submit)
    );
    assert_eq!(
        decode_event(12, String::new(), 0.0, 0.0, 0, 0),
        None,
        "past the table"
    );
    assert_eq!(decode_event(u32::MAX, String::new(), 0.0, 0.0, 0, 0), None);
}

#[quench_test::test]
fn buffers_can_be_allocated_and_freed_including_empty_ones() {
    let empty = q_alloc(0);
    assert!(!empty.is_null());
    unsafe { q_dealloc(empty, 0) };
    let ptr = q_alloc(64);
    assert!(!ptr.is_null());
    unsafe { q_dealloc(ptr, 64) };
    unsafe { q_dealloc(std::ptr::null_mut(), 8) }; // tolerated, not freed
}

fn record_inputs() -> (
    quench_ui_web::Node,
    Rc<RefCell<Vec<String>>>,
    quench_ui::Mount<quench_ui_web::DomRenderer>,
) {
    use quench_ui::prelude::*;
    mock::reset();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let s = seen.clone();
    let m =
        quench_ui_web::mount_root(input().on_input(move |v| s.borrow_mut().push(v.to_string())));
    let node = quench_ui_web::DomRenderer::root();
    (node, seen, m)
}

#[quench_test::test]
fn invalid_utf8_from_js_becomes_replacement_characters() {
    let (_, seen, m) = record_inputs();
    let h = mock::handles_by_tag("input")[0];
    let bad = [b'o', b'k', 0xff, 0xfe, b'!'];
    let ptr = q_alloc(bad.len());
    unsafe {
        std::ptr::copy_nonoverlapping(bad.as_ptr(), ptr, bad.len());
        q_dispatch_event(h, 2, ptr, bad.len(), 0.0, 0.0, 0, 0);
    }
    assert_eq!(*seen.borrow(), ["ok\u{fffd}\u{fffd}!"]);
    m.unmount();
}

#[quench_test::test]
fn malformed_calls_are_ignored_without_panicking() {
    let (_, seen, m) = record_inputs();
    let h = mock::handles_by_tag("input")[0];
    unsafe {
        q_dispatch_event(h, 999, std::ptr::null_mut(), 0, 0.0, 0.0, 0, 0); // unknown kind
        q_dispatch_event(0, 2, std::ptr::null_mut(), 0, 0.0, 0.0, 0, 0); // handle 0
        q_dispatch_event(123_456, 2, std::ptr::null_mut(), 0, 0.0, 0.0, 0, 0); // never created
        q_dispatch_event(h, 0, std::ptr::null_mut(), 0, 0.0, 0.0, 0, 0); // a kind with no listener
        q_dispatch_event(
            h,
            2,
            std::ptr::null_mut(),
            0,
            f64::NAN,
            f64::INFINITY,
            i32::MIN,
            u32::MAX,
        ); // empty text, odd numbers
        q_fetch_done(77, 200, std::ptr::null_mut(), 0, std::ptr::null_mut(), 0); // nobody is waiting
        q_fetch_done(
            78,
            u32::MAX,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            0,
        );
    }
    assert_eq!(
        *seen.borrow(),
        [""],
        "only the one valid dispatch (empty value) arrived"
    );
    q_dispatch_input_after_unmount(m);
}

fn q_dispatch_input_after_unmount(m: quench_ui::Mount<quench_ui_web::DomRenderer>) {
    m.unmount();
    unsafe { q_dispatch_event(2, 2, std::ptr::null_mut(), 0, 0.0, 0.0, 0, 0) };
}

#[quench_test::test]
fn the_tick_and_timer_exports_tolerate_nothing_to_do() {
    mock::reset();
    quench_ui_web::q_tick();
    quench_ui_web::q_timer_fire(12345);
    assert_eq!(quench_ui_web::pending_tasks(), 0);
}
