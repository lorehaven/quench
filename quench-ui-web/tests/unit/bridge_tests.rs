//! The bridge to the host app's plugins, with the mock host playing the host.

use quench_serde::{Deserialize, Serialize, Value, json};
use quench_ui_web::bridge;
use quench_ui_web::mock;
use quench_ui_web::pending_fetches;
use quench_ui_web::{BridgeError, spawn};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Device {
    sdk: u32,
    model: String,
}

#[quench_ui_web::plugin("echo")]
pub trait Echo {
    /// Returns the text.
    async fn say(text: String) -> Result<String, BridgeError>;
    async fn device() -> Result<Device, BridgeError>;
    async fn add(a: i64, b: i64) -> Result<i64, BridgeError>;
}

type Slot<T> = Rc<RefCell<Option<Result<T, BridgeError>>>>;

fn run<T: 'static>(
    f: impl std::future::Future<Output = Result<T, BridgeError>> + 'static,
) -> Slot<T> {
    let slot: Slot<T> = Rc::new(RefCell::new(None));
    let s = slot.clone();
    spawn(async move { *s.borrow_mut() = Some(f.await) });
    mock::run_ticks();
    slot
}

fn sent() -> Vec<Value> {
    mock::bridge_sent()
        .iter()
        .map(|t| json::from_str(t).unwrap())
        .collect()
}

#[quench_test::test]
fn without_a_host_a_call_fails_with_unavailable() {
    mock::reset();
    let slot = run(Echo::say("hi".into()));
    assert!(slot.borrow().is_none(), "the page decides, not the module");
    assert_eq!(
        mock::bridge_sent().len(),
        1,
        "a call is a fetch of quench-bridge:call"
    );
    mock::bridge_unavailable(); // what quench.js does in a page that has no host
    let err = slot.borrow_mut().take().unwrap().unwrap_err();
    assert_eq!(err.code, "unavailable");
    assert_eq!(pending_fetches(), 0);
}

#[quench_test::test]
fn a_call_is_an_envelope_with_the_arguments_by_name() {
    mock::reset();
    let _slot = run(Echo::add(2, -3));
    let envelopes = sent();
    assert_eq!(envelopes.len(), 1);
    let e = &envelopes[0];
    assert_eq!(e.get("plugin").and_then(Value::as_str), Some("echo"));
    assert_eq!(e.get("method").and_then(Value::as_str), Some("add"));
    assert_eq!(e.get("args"), Some(&json!({ "a": 2, "b": -3 })));
    assert!(e.get("id").and_then(Value::as_u64).is_some());
    assert_eq!(pending_fetches(), 1);
}

#[quench_test::test]
fn the_answer_with_the_same_id_resolves_the_call() {
    mock::reset();
    let slot = run(Echo::say("hi".into()));
    assert!(slot.borrow().is_none(), "waiting for the host");
    let id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!("{{\"id\":{id},\"ok\":\"hi\"}}"));
    assert_eq!(slot.borrow_mut().take().unwrap().unwrap(), "hi");
    assert_eq!(pending_fetches(), 0);
}

#[quench_test::test]
fn structured_results_are_read_into_their_type() {
    mock::reset();
    let slot = run(Echo::device());
    let id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!(
        "{{\"id\":{id},\"ok\":{{\"sdk\":36,\"model\":\"sdk_gphone\"}}}}"
    ));
    assert_eq!(
        slot.borrow_mut().take().unwrap().unwrap(),
        Device {
            sdk: 36,
            model: "sdk_gphone".into()
        }
    );
}

#[quench_test::test]
fn an_error_answer_keeps_its_code_and_message() {
    mock::reset();
    let slot = run(Echo::say("hi".into()));
    let id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!(
        "{{\"id\":{id},\"err\":{{\"code\":\"permission_denied\",\"message\":\"no\"}}}}"
    ));
    let err = slot.borrow_mut().take().unwrap().unwrap_err();
    assert_eq!(err, BridgeError::new("permission_denied", "no"));
    assert_eq!(err.to_string(), "permission_denied: no");
}

#[quench_test::test]
fn a_result_of_the_wrong_shape_is_a_bad_response() {
    mock::reset();
    let slot = run(Echo::say("hi".into()));
    let id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!("{{\"id\":{id},\"ok\":42}}"));
    assert_eq!(
        slot.borrow_mut().take().unwrap().unwrap_err().code,
        "bad_response"
    );
}

#[quench_test::test]
fn several_calls_are_answered_out_of_order_by_id() {
    mock::reset();
    let first = run(Echo::say("one".into()));
    let second = run(Echo::say("two".into()));
    let ids: Vec<u64> = sent()
        .iter()
        .map(|e| e.get("id").and_then(Value::as_u64).unwrap())
        .collect();
    assert_ne!(ids[0], ids[1]);
    mock::bridge_deliver(&format!("{{\"id\":{},\"ok\":\"two\"}}", ids[1]));
    assert!(first.borrow().is_none());
    assert_eq!(second.borrow_mut().take().unwrap().unwrap(), "two");
    mock::bridge_deliver(&format!("{{\"id\":{},\"ok\":\"one\"}}", ids[0]));
    assert_eq!(first.borrow_mut().take().unwrap().unwrap(), "one");
}

#[quench_test::test]
fn messages_that_are_not_answers_are_ignored() {
    mock::reset();
    let slot = run(Echo::say("hi".into()));
    for junk in [
        "",
        "not json",
        "[]",
        "{}",
        "{\"id\":\"x\"}",
        "{\"id\":99999999999}",
        "{\"id\":4000,\"ok\":1}",
    ] {
        mock::bridge_deliver(junk);
    }
    assert!(slot.borrow().is_none(), "still waiting");
    assert_eq!(pending_fetches(), 1);
    let id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!("{{\"id\":{id}}}"));
    assert_eq!(
        slot.borrow_mut().take().unwrap().unwrap_err().code,
        "bad_response",
        "no result at all"
    );
}

#[quench_test::test]
fn dropping_a_call_forgets_it_and_a_late_answer_is_ignored() {
    mock::reset();
    let id;
    {
        let reply = std::pin::pin!(Echo::say("hi".into()));
        let waker = std::task::Waker::noop();
        let mut cx = std::task::Context::from_waker(waker);
        assert!(std::future::Future::poll(reply, &mut cx).is_pending());
        id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
        assert_eq!(pending_fetches(), 1);
    }
    assert_eq!(pending_fetches(), 0);
    mock::bridge_deliver(&format!("{{\"id\":{id},\"ok\":\"late\"}}"));
    assert_eq!(pending_fetches(), 0);
}

#[quench_test::test]
fn the_generated_client_knows_its_plugin() {
    assert_eq!(Echo::NAME, "echo");
}

#[quench_test::test]
fn events_pushed_by_the_host_reach_every_listener() {
    mock::reset();
    let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let (a, b) = (seen.clone(), seen.clone());
    bridge::on_event(move |plugin, name, data| {
        a.borrow_mut()
            .push(format!("a {plugin}.{name} {}", json::to_string(&data)));
    });
    bridge::on_event(move |_, name, _| {
        b.borrow_mut().push(format!("b {name}"));
    });
    mock::bridge_deliver("{\"event\":\"progress\",\"plugin\":\"files\",\"data\":{\"sent\":5}}");
    mock::bridge_deliver("{\"event\":\"ping\"}");
    assert_eq!(
        *seen.borrow(),
        [
            "a files.progress {\"sent\":5}",
            "b progress",
            "a .ping null",
            "b ping"
        ]
    );
}

#[quench_test::test]
fn an_event_is_not_an_answer_and_an_answer_is_not_an_event() {
    mock::reset();
    let heard = Rc::new(RefCell::new(0));
    let h = heard.clone();
    bridge::on_event(move |_, _, _| *h.borrow_mut() += 1);
    let slot = run(Echo::say("hi".into()));
    let id = sent()[0].get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!("{{\"id\":{id},\"ok\":\"hi\"}}"));
    assert_eq!(*heard.borrow(), 0);
    assert!(slot.borrow().is_some());
    mock::reset();
    mock::bridge_deliver("{\"event\":\"late\"}");
    assert_eq!(*heard.borrow(), 0, "a reset forgets the listeners");
}
