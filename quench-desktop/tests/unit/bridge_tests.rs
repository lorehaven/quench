use quench_desktop::{Bridge, Call, Deliver, Emitter, MAX_MESSAGE, Plugin, string_arg};
use quench_serde::{Value, json};
use std::sync::{Arc, Mutex};

struct Say {
    emitter: Mutex<Option<Emitter>>,
}

impl Plugin for Say {
    fn name(&self) -> &'static str {
        "say"
    }

    fn call(&self, method: &str, args: &Value, call: Call) {
        match method {
            "echo" => match string_arg(args, "text") {
                Ok(text) => call.resolve(&json!({ "text": text })),
                Err(message) => call.reject("invalid_args", &message),
            },
            "later" => {
                // Answered from another thread, as a plugin that waits for the system does.
                std::thread::spawn(move || call.resolve(&json!("late")));
            }
            "ping" => {
                if let Some(emitter) = self.emitter.lock().unwrap().as_ref() {
                    emitter.emit("say", "pinged", &json!({ "n": 1 }));
                }
                call.resolve(&Value::Null);
            }
            "boom" => panic!("a plugin bug"),
            "debug" => {
                let shown = format!("{call:?}");
                call.resolve(&json!(shown));
            }
            _ => call.reject("unknown_method", method),
        }
    }

    fn attach(&self, emitter: Emitter) {
        *self.emitter.lock().unwrap() = Some(emitter);
    }
}

fn bridge() -> (Bridge, Arc<Mutex<Vec<Value>>>) {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&sent);
    let deliver: Deliver = Arc::new(move |text: String| {
        sink.lock()
            .unwrap()
            .push(quench_serde::json::from_str(&text).unwrap());
    });
    let plugin = Say {
        emitter: Mutex::new(None),
    };
    (Bridge::new(vec![Arc::new(plugin)], deliver), sent)
}

#[quench_test::test]
fn a_call_is_answered_with_its_id_and_result() {
    let (bridge, sent) = bridge();
    bridge.handle(r#"{"id":7,"plugin":"say","method":"echo","args":{"text":"hi"}}"#);
    assert_eq!(
        sent.lock().unwrap()[0],
        json!({ "id": 7, "ok": { "text": "hi" } })
    );
    // Ids are whatever the page chose.
    bridge.handle(r#"{"id":"a-1","plugin":"say","method":"echo","args":{"text":"x"}}"#);
    assert_eq!(sent.lock().unwrap()[1]["id"], json!("a-1"));
}

#[quench_test::test]
fn mistakes_are_rejected_with_a_code() {
    let (bridge, sent) = bridge();
    bridge.handle(r#"{"id":1,"plugin":"nobody","method":"x"}"#);
    bridge.handle(r#"{"id":2,"plugin":"say","method":"nope"}"#);
    bridge.handle(r#"{"id":3,"plugin":"say","method":"echo","args":{}}"#);
    bridge.handle(r#"{"id":4,"plugin":"say","method":"echo","args":"not an object"}"#);
    let sent = sent.lock().unwrap().clone();
    let codes: Vec<_> = sent
        .iter()
        .map(|m| m["err"]["code"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        codes,
        [
            "unknown_plugin",
            "unknown_method",
            "invalid_args",
            "invalid_args"
        ]
    );
    assert!(
        sent[0]["err"]["message"]
            .as_str()
            .unwrap()
            .contains("nobody")
    );
}

#[quench_test::test]
fn a_call_can_be_printed() {
    let (bridge, sent) = bridge();
    bridge.handle(r#"{"id":5,"plugin":"say","method":"debug"}"#);
    assert!(
        sent.lock().unwrap()[0]["ok"]
            .as_str()
            .unwrap()
            .contains("Call { id: UInt(5)")
    );
}

#[quench_test::test]
fn what_cannot_be_answered_is_dropped() {
    let (bridge, sent) = bridge();
    bridge.handle("not json");
    bridge.handle(r#"{"plugin":"say","method":"echo"}"#); // no id
    bridge.handle(r#"{"id":null,"plugin":"say","method":"echo"}"#);
    bridge.handle(&format!(
        r#"{{"id":1,"plugin":"say","method":"echo","args":{{"text":"{}"}}}}"#,
        "x".repeat(MAX_MESSAGE)
    ));
    assert!(sent.lock().unwrap().is_empty());
}

#[quench_test::test]
fn a_plugin_that_panics_is_answered_internal_and_the_bridge_goes_on() {
    let (bridge, sent) = bridge();
    bridge.handle(r#"{"id":1,"plugin":"say","method":"boom"}"#);
    bridge.handle(r#"{"id":2,"plugin":"say","method":"echo","args":{"text":"still here"}}"#);
    let sent = sent.lock().unwrap();
    assert_eq!(sent[0]["err"]["code"], json!("internal"));
    assert_eq!(sent[1]["ok"]["text"], json!("still here"));
}

#[quench_test::test]
fn answers_may_come_later_from_another_thread_and_events_are_pushed() {
    let (bridge, sent) = bridge();
    bridge.handle(r#"{"id":1,"plugin":"say","method":"later"}"#);
    for _ in 0..200 {
        if !sent.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(sent.lock().unwrap()[0], json!({ "id": 1, "ok": "late" }));
    bridge.handle(r#"{"id":2,"plugin":"say","method":"ping"}"#);
    let sent = sent.lock().unwrap();
    assert_eq!(
        sent[1],
        json!({ "event": "pinged", "plugin": "say", "data": { "n": 1 } })
    );
    assert_eq!(sent[2], json!({ "id": 2, "ok": null }));
    drop(sent);
    assert!(format!("{bridge:?}").contains("say"));
    assert!(format!("{:?}", bridge.emitter()).contains("Emitter"));
}
