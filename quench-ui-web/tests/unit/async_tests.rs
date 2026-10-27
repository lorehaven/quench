//! The executor, `sleep` and `fetch`, with the host playing the browser.

use quench_ui_web::mock::{self, Call};
use quench_ui_web::{FetchError, Request, pending_fetches, pending_tasks, sleep, spawn};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[quench_test::test]
fn a_spawned_task_runs_when_the_host_ticks() {
    mock::reset();
    let ran = Rc::new(Cell::new(false));
    let r = ran.clone();
    spawn(async move { r.set(true) });
    assert!(!ran.get(), "spawn only queues the task");
    assert_eq!(pending_tasks(), 1);
    assert!(mock::run_ticks(), "and asks the host for a tick");
    assert!(ran.get());
    assert_eq!(pending_tasks(), 0);
    assert!(!mock::run_ticks(), "nothing asked again");
}

#[quench_test::test]
fn many_spawns_ask_for_one_tick() {
    mock::reset();
    let n = Rc::new(Cell::new(0));
    for _ in 0..5 {
        let n = n.clone();
        spawn(async move { n.set(n.get() + 1) });
    }
    assert!(mock::run_ticks());
    assert_eq!(n.get(), 5);
}

#[quench_test::test]
fn a_task_can_spawn_more_tasks_in_the_same_tick() {
    mock::reset();
    let order = Rc::new(RefCell::new(Vec::new()));
    let o = order.clone();
    spawn(async move {
        o.borrow_mut().push("outer");
        let o2 = o.clone();
        spawn(async move { o2.borrow_mut().push("inner") });
    });
    mock::run_ticks();
    assert_eq!(*order.borrow(), ["outer", "inner"]);
}

#[quench_test::test]
fn sleep_waits_for_the_host_timer() {
    mock::reset();
    let done = Rc::new(Cell::new(false));
    let d = done.clone();
    spawn(async move {
        sleep(250).await;
        d.set(true);
    });
    mock::run_ticks();
    let timers = mock::timers();
    assert_eq!(timers.len(), 1);
    assert_eq!(timers[0].1, 250);
    assert!(!done.get());
    assert!(mock::fire_timer(timers[0].0));
    assert!(done.get());
    assert!(mock::timers().is_empty());
    assert!(!mock::fire_timer(timers[0].0), "a timer fires once");
}

#[quench_test::test]
fn sequential_sleeps_and_dropped_sleeps() {
    mock::reset();
    let steps = Rc::new(RefCell::new(Vec::new()));
    let s = steps.clone();
    spawn(async move {
        sleep(10).await;
        s.borrow_mut().push(1);
        sleep(20).await;
        s.borrow_mut().push(2);
    });
    mock::run_ticks();
    mock::fire_timer(mock::timers()[0].0);
    assert_eq!(*steps.borrow(), [1]);
    mock::fire_timer(mock::timers()[0].0);
    assert_eq!(*steps.borrow(), [1, 2]);

    // A sleep that is dropped while pending clears its timer.
    mock::clear_calls();
    spawn(async {
        let pending = sleep(1000);
        let mut pending = Box::pin(pending);
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        assert!(std::future::Future::poll(pending.as_mut(), &mut cx).is_pending());
        drop(pending);
    });
    mock::run_ticks();
    let calls = mock::calls();
    assert!(calls.iter().any(|c| matches!(c, Call::SetTimeout(_, 1000))));
    assert!(calls.iter().any(|c| matches!(c, Call::ClearTimeout(_))));
    assert!(mock::timers().is_empty());
    let _ = format!("{:?}", sleep(1));
}

#[quench_test::test]
fn a_finished_sleep_that_is_dropped_does_not_clear_again() {
    mock::reset();
    spawn(async { sleep(5).await });
    mock::run_ticks();
    mock::fire_timer(mock::timers()[0].0);
    assert!(
        !mock::calls()
            .iter()
            .any(|c| matches!(c, Call::ClearTimeout(_)))
    );
}

#[quench_test::test]
fn fetch_sends_the_request_and_resolves_with_the_response() {
    mock::reset();
    let got = Rc::new(RefCell::new(None));
    let g = got.clone();
    spawn(async move {
        let r = Request::post("/api/x")
            .header("content-type", "application/json")
            .header("x-bad", "a\nb") // dropped: would break the one-per-line format
            .header("bad:name", "v") // dropped
            .body("{\"a\":1}")
            .send()
            .await;
        *g.borrow_mut() = Some(r);
    });
    mock::run_ticks();
    let fetches = mock::fetches();
    assert_eq!(fetches.len(), 1);
    assert_eq!(
        (fetches[0].method.as_str(), fetches[0].url.as_str()),
        ("POST", "/api/x")
    );
    assert_eq!(fetches[0].headers, "content-type: application/json");
    assert_eq!(fetches[0].body, b"{\"a\":1}");
    assert_eq!(pending_fetches(), 1);
    assert!(mock::complete_fetch(
        fetches[0].id,
        201,
        b"created",
        "Content-Type: text/plain\nx-n:  5 \nnot a header"
    ));
    let response = got.borrow_mut().take().unwrap().unwrap();
    assert_eq!(response.status, 201);
    assert!(response.ok());
    assert_eq!(response.text(), "created");
    assert_eq!(response.header("content-type"), Some("text/plain"));
    assert_eq!(response.header("X-N"), Some("5"));
    assert_eq!(response.header("missing"), None);
    assert_eq!(pending_fetches(), 0);
}

#[quench_test::test]
fn error_statuses_are_responses_but_status_zero_is_an_error() {
    mock::reset();
    let results = Rc::new(RefCell::new(Vec::new()));
    let r = results.clone();
    spawn(async move {
        let a = Request::get("/missing").send().await;
        let b = Request::new("get", "/down").send().await;
        r.borrow_mut().push(a.map(|x| (x.status, x.ok())));
        r.borrow_mut().push(b.map(|x| (x.status, x.ok())));
    });
    mock::run_ticks();
    assert_eq!(mock::fetches()[0].method, "GET");
    mock::complete_fetch(mock::fetches()[0].id, 404, b"nope", "");
    mock::complete_fetch(mock::fetches()[0].id, 0, b"Failed to fetch", "");
    assert_eq!(
        *results.borrow(),
        [
            Ok((404, false)),
            Err(FetchError::Network("Failed to fetch".into()))
        ]
    );
    assert_eq!(
        FetchError::Network("x".into()).to_string(),
        "network error: x"
    );
    assert_eq!(FetchError::Aborted.to_string(), "request aborted");
}

#[quench_test::test]
fn an_abort_handle_resolves_the_future_with_aborted() {
    mock::reset();
    let out = Rc::new(RefCell::new(None));
    let o = out.clone();
    spawn(async move {
        let fetch = Request::get("/slow").send();
        let abort = fetch.abort_handle();
        spawn(async move {
            abort.abort();
            abort.abort();
        });
        *o.borrow_mut() = Some(fetch.await);
    });
    mock::run_ticks();
    assert!(
        mock::calls()
            .iter()
            .filter(|c| matches!(c, Call::FetchAbort(_)))
            .count()
            == 1,
        "a second abort does nothing"
    );
    assert_eq!(
        out.borrow_mut().take().unwrap().unwrap_err(),
        FetchError::Aborted
    );
    assert!(format!("{:?}", Request::get("/").send()).contains("Fetch"));
}

#[quench_test::test]
fn aborting_a_finished_fetch_is_a_no_op_and_dropping_a_pending_one_aborts() {
    mock::reset();
    let handle = Rc::new(RefCell::new(None));
    let h = handle.clone();
    spawn(async move {
        let fetch = Request::get("/fast").send();
        *h.borrow_mut() = Some(fetch.abort_handle());
        let _ = fetch.await;
    });
    mock::run_ticks();
    mock::complete_fetch(mock::fetches()[0].id, 200, b"", "");
    mock::clear_calls();
    handle.borrow().unwrap().abort();
    assert!(mock::calls().is_empty());

    // Dropping the future before it completes aborts the request; its late completion is ignored.
    spawn(async {
        let fetch = Request::get("/dropped").send();
        drop(fetch);
    });
    mock::run_ticks();
    assert!(
        mock::calls()
            .iter()
            .any(|c| matches!(c, Call::FetchAbort(_)))
    );
    assert_eq!(pending_fetches(), 0);
    assert!(!mock::complete_fetch(9999, 200, b"late", ""));
    assert_eq!(Request::get("/a"), Request::get("/a"));
}

#[quench_test::test]
fn a_status_larger_than_u16_is_clamped_and_text_is_lossy() {
    mock::reset();
    let out = Rc::new(RefCell::new(None));
    let o = out.clone();
    spawn(async move { *o.borrow_mut() = Some(Request::get("/x").send().await) });
    mock::run_ticks();
    mock::complete_fetch(mock::fetches()[0].id, 70_000, &[b'a', 0xff], "");
    let r = out.borrow_mut().take().unwrap().unwrap();
    assert_eq!(r.status, u16::MAX);
    assert_eq!(r.text(), "a\u{fffd}");
}
