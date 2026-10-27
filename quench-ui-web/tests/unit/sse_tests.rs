//! `EventSource`: opening, the events it reports, closing, and what happens when it goes away.

use quench_ui_web::mock::{self, Call};
use quench_ui_web::{EventSource, SseEvent, spawn};
use std::cell::RefCell;
use std::rc::Rc;

fn message(name: &str, data: &str) -> SseEvent {
    SseEvent::Message {
        name: name.to_string(),
        data: data.to_string(),
    }
}

/// Runs a task that collects what `source` reports until it ends.
fn collect(source: EventSource) -> Rc<RefCell<Vec<SseEvent>>> {
    let seen: Rc<RefCell<Vec<SseEvent>>> = Rc::default();
    let out = seen.clone();
    spawn(async move {
        while let Some(event) = source.next().await {
            out.borrow_mut().push(event);
        }
        out.borrow_mut().push(SseEvent::Closed); // the loop ended
    });
    mock::run_ticks();
    seen
}

#[quench_test::test]
fn opening_tells_the_browser_the_url_and_the_event_names() {
    mock::reset();
    let _source = EventSource::open("/api/stream", &["note", "tick", "bad\nname"]);
    let sources = mock::sse_sources();
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].1, "/api/stream");
    assert_eq!(
        sources[0].2,
        ["note", "tick"],
        "a name with a newline would break the list"
    );
    assert!(mock::calls().contains(&Call::SseOpen(sources[0].0)));
}

#[quench_test::test]
fn events_arrive_in_order_and_wake_a_waiting_task() {
    mock::reset();
    let source = EventSource::open("/s", &["note"]);
    let id = mock::sse_sources()[0].0;
    let seen = collect(source);
    assert!(
        seen.borrow().is_empty(),
        "nothing yet, and the task is waiting"
    );

    assert!(mock::sse_emit(id, 0, "", ""));
    assert!(mock::sse_emit(id, 1, "note", "first"));
    assert!(mock::sse_emit(id, 1, "message", "unnamed"));
    assert!(mock::sse_emit(id, 1, "note", "multi\nline \u{1f600} <b>"));
    assert_eq!(
        *seen.borrow(),
        [
            SseEvent::Open,
            message("note", "first"),
            message("message", "unnamed"),
            message("note", "multi\nline \u{1f600} <b>")
        ]
    );
}

#[quench_test::test]
fn only_the_events_asked_for_are_reported() {
    mock::reset();
    let _source = EventSource::open("/s", &["note"]);
    let id = mock::sse_sources()[0].0;
    assert!(
        !mock::sse_emit(id, 1, "other", "x"),
        "the browser has no listener for it"
    );
    assert!(
        mock::sse_emit(id, 1, "message", "x"),
        "the unnamed event always has one"
    );
    assert!(
        !mock::sse_emit(id + 100, 1, "note", "x"),
        "and an unknown source gets nothing"
    );
}

#[quench_test::test]
fn losing_the_connection_is_reported_and_the_stream_goes_on() {
    mock::reset();
    let source = EventSource::open("/s", &["note"]);
    let id = mock::sse_sources()[0].0;
    let seen = collect(source);
    mock::sse_emit(id, 0, "", "");
    mock::sse_emit(id, 2, "", ""); // the browser is reconnecting
    mock::sse_emit(id, 0, "", "");
    mock::sse_emit(id, 1, "note", "after");
    assert_eq!(
        *seen.borrow(),
        [
            SseEvent::Open,
            SseEvent::Reconnecting,
            SseEvent::Open,
            message("note", "after")
        ]
    );
}

#[quench_test::test]
fn a_stream_that_fails_for_good_ends_after_its_last_event() {
    mock::reset();
    let source = EventSource::open("/s", &[]);
    let id = mock::sse_sources()[0].0;
    let seen = collect(source);
    mock::sse_emit(id, 1, "message", "last");
    mock::sse_emit(id, 3, "", "");
    // `Closed` is the last event, then the loop ends (the task pushes its own marker).
    assert_eq!(
        *seen.borrow(),
        [
            message("message", "last"),
            SseEvent::Closed,
            SseEvent::Closed
        ]
    );
    assert!(
        !mock::sse_emit(id, 1, "message", "ignored"),
        "nothing after a failure is delivered"
    );
}

#[quench_test::test]
fn closing_stops_the_browser_and_ends_the_stream() {
    mock::reset();
    let source = Rc::new(EventSource::open("/s", &["note"]));
    let id = mock::sse_sources()[0].0;
    mock::sse_emit(id, 1, "note", "kept");
    let seen: Rc<RefCell<Vec<SseEvent>>> = Rc::default();
    let (reader, out) = (source.clone(), seen.clone());
    spawn(async move {
        while let Some(event) = reader.next().await {
            out.borrow_mut().push(event);
        }
    });
    mock::run_ticks();
    assert_eq!(*seen.borrow(), [message("note", "kept")]);
    source.close();
    source.close(); // idempotent
    mock::run_ticks();
    assert!(
        mock::sse_sources().is_empty(),
        "the browser's source is gone"
    );
    assert_eq!(
        mock::calls()
            .iter()
            .filter(|c| matches!(c, Call::SseClose(_)))
            .count(),
        2,
        "asked twice, harmless"
    );
    assert!(!mock::sse_emit(id, 1, "note", "late"));
    assert_eq!(quench_ui_web::pending_tasks(), 0, "the reading task ended");
}

#[quench_test::test]
fn dropping_a_source_closes_it() {
    mock::reset();
    {
        let _source = EventSource::open("/s", &["note"]);
        assert_eq!(mock::sse_sources().len(), 1);
    }
    assert!(mock::sse_sources().is_empty());
    assert!(format!("{:?}", EventSource::open("/t", &[])).contains("EventSource"));
}

#[quench_test::test]
fn events_with_an_unknown_kind_are_ignored() {
    mock::reset();
    let source = EventSource::open("/s", &[]);
    let id = mock::sse_sources()[0].0;
    let seen = collect(source);
    mock::sse_emit(id, 99, "", "");
    mock::sse_emit(id, 1, "message", "ok");
    assert_eq!(*seen.borrow(), [message("message", "ok")]);
}

#[quench_test::test]
fn a_source_left_open_when_the_thread_ends_does_not_abort_the_process() {
    mock::reset();
    let source = EventSource::open("/s", &[]);
    spawn(async move {
        let _ = source.next().await; // never completes in this test
    });
    mock::run_ticks();
}
