//! The typed clients of the host's plugins: what each call sends, and that a reply of the shape the
//! Kotlin side makes is read into the right type.

use quench_serde::{Value, json};
use quench_ui_web::mock;
use quench_ui_web::pending_fetches;
use quench_ui_web::plugins::*;
use quench_ui_web::{BridgeError, spawn};
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

/// Runs `call`, checks the envelope it sent, answers with `reply` (the JSON of `ok`) and returns
/// what the call resolved to.
fn exchange<T: 'static>(
    call: impl Future<Output = Result<T, BridgeError>> + 'static,
    plugin: &str,
    method: &str,
    args: &str,
    reply: &str,
) -> Result<T, BridgeError> {
    mock::reset();
    let slot: Rc<RefCell<Option<Result<T, BridgeError>>>> = Rc::new(RefCell::new(None));
    let s = slot.clone();
    spawn(async move { *s.borrow_mut() = Some(call.await) });
    mock::run_ticks();
    let sent = mock::bridge_sent();
    assert_eq!(sent.len(), 1, "one envelope for {plugin}.{method}");
    let envelope: Value = quench_serde::json::from_str(&sent[0]).unwrap();
    assert_eq!(envelope.get("plugin").and_then(Value::as_str), Some(plugin));
    assert_eq!(envelope.get("method").and_then(Value::as_str), Some(method));
    let want: Value = quench_serde::json::from_str(args).unwrap();
    assert_eq!(
        envelope.get("args"),
        Some(&want),
        "the arguments of {plugin}.{method}"
    );
    let id = envelope.get("id").and_then(Value::as_u64).unwrap();
    mock::bridge_deliver(&format!("{{\"id\":{id},\"ok\":{reply}}}"));
    assert_eq!(pending_fetches(), 0);
    slot.borrow_mut().take().expect("answered")
}

fn ok<T: 'static>(
    call: impl Future<Output = Result<T, BridgeError>> + 'static,
    plugin: &str,
    method: &str,
    args: &str,
    reply: &str,
) -> T {
    exchange(call, plugin, method, args, reply).unwrap_or_else(|e| panic!("{plugin}.{method}: {e}"))
}

#[quench_test::test]
fn jobs() {
    let r = ok(
        Jobs::schedule("heartbeat".into(), 60, "unmetered".into(), true, false),
        "jobs",
        "schedule",
        r#"{"task":"heartbeat","interval_minutes":60,"network":"unmetered","charging":true,"battery_not_low":false}"#,
        r#"{"scheduled":true}"#,
    );
    assert_eq!(r, Scheduled { scheduled: true });
    assert!(
        ok(
            Jobs::run_now("heartbeat".into()),
            "jobs",
            "run_now",
            r#"{"task":"heartbeat"}"#,
            r#"{"scheduled":true}"#
        )
        .scheduled
    );
    ok(
        Jobs::cancel("heartbeat".into()),
        "jobs",
        "cancel",
        r#"{"task":"heartbeat"}"#,
        "null",
    );
    let pending = ok(
        Jobs::pending(),
        "jobs",
        "pending",
        "{}",
        r#"[{"task":"heartbeat","periodic":true,"interval_minutes":15}]"#,
    );
    assert_eq!(
        pending,
        [PendingJob {
            task: "heartbeat".into(),
            periodic: true,
            interval_minutes: 15
        }]
    );
    let last = ok(
        Jobs::last_run("heartbeat".into()),
        "jobs",
        "last_run",
        r#"{"task":"heartbeat"}"#,
        r#"{"runs":3,"at":1793133388861,"result":"done"}"#,
    );
    assert_eq!(
        last,
        LastRun {
            runs: 3,
            at: 1_793_133_388_861,
            result: "done".into()
        }
    );
}

#[quench_test::test]
fn device() {
    let state = ok(
        Device::state(),
        "device",
        "state",
        "{}",
        r#"{"online":true,"transport":"wifi","metered":false,"battery_percent":100,"charging":true,"battery_low":false,"power_save":false}"#,
    );
    assert_eq!(state.transport, "wifi");
    assert!(state.online && state.charging && !state.metered);
    ok(Device::watch(), "device", "watch", "{}", "null");
    ok(Device::unwatch(), "device", "unwatch", "{}", "null");
}

#[quench_test::test]
fn secure_storage() {
    ok(
        SecureStorage::set("token".into(), "s3".into()),
        "secure_storage",
        "set",
        r#"{"key":"token","value":"s3"}"#,
        "null",
    );
    assert_eq!(
        ok(
            SecureStorage::get("token".into()),
            "secure_storage",
            "get",
            r#"{"key":"token"}"#,
            r#""s3""#
        ),
        Some("s3".to_string())
    );
    assert_eq!(
        ok(
            SecureStorage::get("none".into()),
            "secure_storage",
            "get",
            r#"{"key":"none"}"#,
            "null"
        ),
        None,
        "absent is None"
    );
    ok(
        SecureStorage::remove("token".into()),
        "secure_storage",
        "remove",
        r#"{"key":"token"}"#,
        "null",
    );
    assert_eq!(
        ok(
            SecureStorage::keys(),
            "secure_storage",
            "keys",
            "{}",
            r#"["a","b"]"#
        ),
        ["a", "b"]
    );
    ok(
        SecureStorage::clear(),
        "secure_storage",
        "clear",
        "{}",
        "null",
    );
    let corrupt = exchange(
        SecureStorage::get("t".into()),
        "secure_storage",
        "get",
        r#"{"key":"t"}"#,
        "null",
    );
    assert!(corrupt.is_ok());
}

#[quench_test::test]
fn notifications() {
    ok(
        Notifications::channel("r".into(), "Reminders".into(), 3, "d".into()),
        "notifications",
        "channel",
        r#"{"id":"r","name":"Reminders","importance":3,"description":"d"}"#,
        "null",
    );
    assert!(
        ok(
            Notifications::show(1, "Hi".into(), "there".into(), "r".into()),
            "notifications",
            "show",
            r#"{"id":1,"title":"Hi","body":"there","channel":"r"}"#,
            r#"{"posted":true}"#
        )
        .posted
    );
    // A reminder that does not repeat sends null for the zone and time: the host reads null as absent.
    let armed = ok(
        Notifications::schedule(7, 1000, "T".into(), "B".into(), "r".into(), None, None),
        "notifications",
        "schedule",
        r#"{"id":7,"at_ms":1000,"title":"T","body":"B","channel":"r","repeat_zone":null,"repeat_time":null}"#,
        r#"{"exact":true,"at_ms":1000}"#,
    );
    assert_eq!(
        armed,
        Armed {
            exact: true,
            at_ms: 1000
        }
    );
    ok(
        Notifications::schedule(
            9,
            0,
            "D".into(),
            "B".into(),
            "r".into(),
            Some("Asia/Tokyo".into()),
            Some("06:00".into()),
        ),
        "notifications",
        "schedule",
        r#"{"id":9,"at_ms":0,"title":"D","body":"B","channel":"r","repeat_zone":"Asia/Tokyo","repeat_time":"06:00"}"#,
        r#"{"exact":false,"at_ms":5}"#,
    );
    ok(
        Notifications::cancel(9),
        "notifications",
        "cancel",
        r#"{"id":9}"#,
        "null",
    );
    let pending = ok(
        Notifications::pending(),
        "notifications",
        "pending",
        "{}",
        r#"[{"id":9,"title":"D","body":"B","at_ms":5,"channel":"r","repeat_zone":"Asia/Tokyo","repeat_time":"06:00"},{"id":1,"title":"T","body":"B","at_ms":2}]"#,
    );
    assert_eq!(pending[0].repeat_zone.as_deref(), Some("Asia/Tokyo"));
    assert_eq!(pending[1].repeat_zone, None, "absent fields are None");
    assert!(
        ok(
            Notifications::permission(),
            "notifications",
            "permission",
            "{}",
            r#"{"granted":true}"#
        )
        .granted
    );
}

#[quench_test::test]
fn files() {
    let media = ok(
        Files::media("images".into(), 5, 0),
        "files",
        "media",
        r#"{"kind":"images","limit":5,"offset":0}"#,
        r#"[{"uri":"content://media/1","name":"a.jpg","size":3,"mime":"image/jpeg","modified_ms":9}]"#,
    );
    assert_eq!(media[0].name, "a.jpg");
    assert_eq!(
        ok(
            Files::pick_folder(),
            "files",
            "pick_folder",
            "{}",
            r#"{"uris":["content://tree/1"]}"#
        )
        .uris,
        ["content://tree/1"]
    );
    ok(
        Files::pick_files(true),
        "files",
        "pick_files",
        r#"{"multiple":true}"#,
        r#"{"uris":[]}"#,
    );
    let entries = ok(
        Files::list_folder("content://tree/1".into(), 10),
        "files",
        "list_folder",
        r#"{"uri":"content://tree/1","limit":10}"#,
        r#"[{"uri":"u","name":"note.txt","mime":"text/plain","is_dir":false,"size":10,"modified_ms":0}]"#,
    );
    assert!(!entries[0].is_dir);
    let stat = ok(
        Files::stat("u".into()),
        "files",
        "stat",
        r#"{"uri":"u"}"#,
        r#"{"name":"a","size":5,"mime":null}"#,
    );
    assert_eq!(
        stat,
        FileStat {
            name: "a".into(),
            size: 5,
            mime: None
        }
    );
    let chunk = ok(
        Files::read("u".into(), 0, 16),
        "files",
        "read",
        r#"{"uri":"u","offset":0,"length":16}"#,
        r#"{"data":"aGk=","size":2,"eof":true}"#,
    );
    assert!(chunk.eof && chunk.data == "aGk=");
    let up = ok(
        Files::upload(
            "u1".into(),
            "u".into(),
            "http://x".into(),
            "PUT".into(),
            json!({ "x-a": "b" }),
        ),
        "files",
        "upload",
        r#"{"id":"u1","uri":"u","url":"http://x","method":"PUT","headers":{"x-a":"b"}}"#,
        r#"{"id":"u1","total":20}"#,
    );
    assert_eq!(
        up,
        Upload {
            id: "u1".into(),
            total: 20
        }
    );
    ok(
        Files::cancel_upload("u1".into()),
        "files",
        "cancel_upload",
        r#"{"id":"u1"}"#,
        "null",
    );
}

#[quench_test::test]
fn packages() {
    assert!(
        ok(
            Packages::can_install(),
            "packages",
            "can_install",
            "{}",
            r#"{"allowed":true}"#
        )
        .allowed
    );
    ok(
        Packages::open_install_settings(),
        "packages",
        "open_install_settings",
        "{}",
        "null",
    );
    let gone = ok(
        Packages::installed("a.b".into()),
        "packages",
        "installed",
        r#"{"package":"a.b"}"#,
        r#"{"installed":false}"#,
    );
    assert_eq!(
        gone,
        Installed {
            installed: false,
            version_name: None,
            version_code: None
        }
    );
    let there = ok(
        Packages::installed("a.b".into()),
        "packages",
        "installed",
        r#"{"package":"a.b"}"#,
        r#"{"installed":true,"version_name":"1.2","version_code":7}"#,
    );
    assert_eq!(there.version_code, Some(7));
    ok(
        Packages::install_from_url("http://x/a.apk".into()),
        "packages",
        "install_from_url",
        r#"{"url":"http://x/a.apk"}"#,
        "null",
    );
    ok(
        Packages::install("content://a".into()),
        "packages",
        "install",
        r#"{"uri":"content://a"}"#,
        "null",
    );
}

#[quench_test::test]
fn media_share_and_widgets() {
    assert_eq!(
        ok(
            Tts::speak("hi".into(), "en-US".into(), 1.0, 1.5),
            "tts",
            "speak",
            r#"{"text":"hi","lang":"en-US","rate":1.0,"pitch":1.5}"#,
            r#"{"id":"u1"}"#
        )
        .id,
        "u1"
    );
    ok(Tts::stop(), "tts", "stop", "{}", "null");
    assert_eq!(
        ok(Tts::voices(), "tts", "voices", "{}", r#"["en-US","pl-PL"]"#),
        ["en-US", "pl-PL"]
    );
    assert_eq!(
        ok(
            Audio::play("u".into()),
            "audio",
            "play",
            r#"{"uri":"u"}"#,
            r#"{"duration_ms":6000}"#
        )
        .duration_ms,
        6000
    );
    ok(Audio::pause(), "audio", "pause", "{}", "null");
    ok(Audio::resume(), "audio", "resume", "{}", "null");
    ok(Audio::stop(), "audio", "stop", "{}", "null");
    ok(Audio::seek(3000), "audio", "seek", r#"{"ms":3000}"#, "null");
    let state = ok(
        Audio::state(),
        "audio",
        "state",
        "{}",
        r#"{"playing":false,"position_ms":3000,"duration_ms":6000}"#,
    );
    assert_eq!(state.position_ms, 3000);
    ok(
        Share::text("hi".into(), "Send".into()),
        "share",
        "text",
        r#"{"text":"hi","title":"Send"}"#,
        "null",
    );
    ok(
        Share::file("u".into(), "image/png".into(), "T".into()),
        "share",
        "file",
        r#"{"uri":"u","mime":"image/png","title":"T"}"#,
        "null",
    );
    ok(
        Share::open("u".into(), "text/plain".into()),
        "share",
        "open",
        r#"{"uri":"u","mime":"text/plain"}"#,
        "null",
    );
    let incoming = ok(
        Intents::take(),
        "intents",
        "take",
        "{}",
        r#"[{"action":"android.intent.action.SEND","type":"text/plain","uri":"","text":"shared"}]"#,
    );
    assert_eq!(incoming[0].text, "shared");
    assert_eq!(incoming[0].r#type, "text/plain");
    ok(
        Widget::set("Today".into(), "3".into()),
        "widget",
        "set",
        r#"{"title":"Today","body":"3"}"#,
        "null",
    );
    assert!(
        ok(
            Widget::pin(),
            "widget",
            "pin",
            "{}",
            r#"{"requested":true}"#
        )
        .requested
    );
    assert_eq!(
        ok(Widget::count(), "widget", "count", "{}", r#"{"count":2}"#).count,
        2
    );
}

#[quench_test::test]
fn the_plugin_names_are_the_ones_the_host_registers() {
    for (name, want) in [
        (Jobs::NAME, "jobs"),
        (Device::NAME, "device"),
        (SecureStorage::NAME, "secure_storage"),
        (Notifications::NAME, "notifications"),
        (Files::NAME, "files"),
        (Packages::NAME, "packages"),
        (Tts::NAME, "tts"),
        (Audio::NAME, "audio"),
        (Share::NAME, "share"),
        (Intents::NAME, "intents"),
        (Widget::NAME, "widget"),
    ] {
        assert_eq!(name, want);
    }
}

#[quench_test::test]
fn a_host_error_reaches_the_caller_with_its_code() {
    mock::reset();
    let slot: Rc<RefCell<Option<Result<Chunk, BridgeError>>>> = Rc::new(RefCell::new(None));
    let s = slot.clone();
    spawn(async move { *s.borrow_mut() = Some(Files::read("u".into(), 0, 1).await) });
    mock::run_ticks();
    let id = quench_serde::json::from_str::<Value>(&mock::bridge_sent()[0])
        .unwrap()
        .get("id")
        .and_then(Value::as_u64)
        .unwrap();
    mock::bridge_deliver(&format!(
        "{{\"id\":{id},\"err\":{{\"code\":\"not_found\",\"message\":\"gone\"}}}}"
    ));
    let e = slot.borrow_mut().take().unwrap().unwrap_err();
    assert_eq!((e.code.as_str(), e.message.as_str()), ("not_found", "gone"));
}
