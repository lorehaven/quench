use quench_desktop::plugins::{
    DeviceState, Echo, Notifications, Output, Runner, SecureStorage, Share, System,
};
use quench_desktop::{Bridge, Deliver, Plugin};
use quench_serde::{Value, json};
use std::collections::VecDeque;
use std::io;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Ran = (String, Vec<String>, Option<Vec<u8>>);

/// A stand-in for the system's programs: records what was run and answers from a script.
#[derive(Default)]
struct Fake {
    ran: Mutex<Vec<Ran>>,
    spawned: Mutex<Vec<(String, Vec<String>)>>,
    answers: Mutex<VecDeque<io::Result<Output>>>,
}

impl Fake {
    fn answering(answers: Vec<io::Result<Output>>) -> Arc<Self> {
        Arc::new(Self {
            answers: Mutex::new(answers.into()),
            ..Self::default()
        })
    }
}

#[allow(clippy::unnecessary_wraps)] // the stand-in's script holds `io::Result`s
fn ok(stdout: &str) -> io::Result<Output> {
    Ok(Output {
        success: true,
        stdout: stdout.to_string(),
        stderr: String::new(),
    })
}

#[allow(clippy::unnecessary_wraps)]
fn failed() -> io::Result<Output> {
    Ok(Output::default())
}

impl Runner for Fake {
    fn run(&self, program: &str, args: &[&str], stdin: Option<&[u8]>) -> io::Result<Output> {
        self.ran.lock().unwrap().push((
            program.to_string(),
            args.iter().map(ToString::to_string).collect(),
            stdin.map(<[u8]>::to_vec),
        ));
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| ok(""))
    }

    fn spawn(&self, program: &str, args: &[&str]) -> io::Result<()> {
        self.spawned.lock().unwrap().push((
            program.to_string(),
            args.iter().map(ToString::to_string).collect(),
        ));
        if program == "missing" {
            return Err(io::Error::new(io::ErrorKind::NotFound, "not found"));
        }
        Ok(())
    }
}

/// A system with no programs: nothing starts.
struct Nothing;
impl Runner for Nothing {
    fn run(&self, _: &str, _: &[&str], _: Option<&[u8]>) -> io::Result<Output> {
        Ok(Output::default())
    }
    fn spawn(&self, _: &str, _: &[&str]) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::NotFound, "gone"))
    }
}

struct Page {
    bridge: Bridge,
    messages: Arc<Mutex<Vec<Value>>>,
}

impl Page {
    fn new(plugin: impl Plugin + 'static) -> Self {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&messages);
        let deliver: Deliver = Arc::new(move |text: String| {
            sink.lock()
                .unwrap()
                .push(quench_serde::json::from_str(&text).unwrap());
        });
        Self {
            bridge: Bridge::new(vec![Arc::new(plugin)], deliver),
            messages,
        }
    }

    /// Calls and waits for the answer to it.
    fn call(&self, plugin: &str, method: &str, args: &Value) -> Value {
        let id = self.messages.lock().unwrap().len() + 1000;
        self.bridge.handle(
            &json!({ "id": id, "plugin": plugin, "method": method, "args": args }).to_string(),
        );
        for _ in 0..300 {
            if let Some(answer) = self
                .messages
                .lock()
                .unwrap()
                .iter()
                .find(|m| m["id"] == json!(id))
            {
                return answer.clone();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no answer to {plugin}.{method}");
    }
}

fn code(answer: &Value) -> &str {
    answer["err"]["code"].as_str().unwrap_or("")
}

#[quench_test::test]
fn echo_says_adds_fails_and_describes_the_machine() {
    let page = Page::new(Echo);
    assert_eq!(
        page.call("echo", "say", &json!({ "text": "hi" }))["ok"],
        json!("hi")
    );
    assert_eq!(
        page.call("echo", "add", &json!({ "a": 2, "b": 40 }))["ok"],
        json!(42)
    );
    assert_eq!(
        code(&page.call("echo", "add", &json!({ "a": 2 }))),
        "invalid_args"
    );
    assert_eq!(code(&page.call("echo", "say", &json!({}))), "invalid_args");
    assert_eq!(
        code(&page.call("echo", "fail", &json!({ "code": "teapot" }))),
        "teapot"
    );
    assert_eq!(code(&page.call("echo", "fail", &json!({}))), "failed");
    assert_eq!(
        page.call("echo", "notifications", &json!({}))["ok"],
        json!(true)
    );
    let device = page.call("echo", "device", &json!({}));
    assert_eq!(device["ok"]["sdk"], json!(0));
    assert!(!device["ok"]["model"].as_str().unwrap().is_empty());
    assert_eq!(
        code(&page.call("echo", "nope", &json!({}))),
        "unknown_method"
    );
}

/// A `/sys`-shaped tree.
fn sys(files: &[(&str, &str)]) -> quench_core::tmp::TempDir {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    for (path, content) in files {
        let target = dir.path().join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, content).unwrap();
    }
    dir
}

#[quench_test::test]
fn the_state_comes_from_sys() {
    let dir = sys(&[
        ("class/net/lo/operstate", "unknown\n"),
        ("class/net/docker0/operstate", "up\n"),
        ("class/net/wlan0/operstate", "up\n"),
        ("class/net/wlan0/device/x", ""),
        ("class/power_supply/BAT0/capacity", "12\n"),
        ("class/power_supply/BAT0/status", "Discharging\n"),
        ("firmware/acpi/platform_profile", "low-power\n"),
    ]);
    let state = Page::new(DeviceState::at(dir.path(), Duration::from_secs(60))).call(
        "device",
        "state",
        &json!({}),
    )["ok"]
        .clone();
    assert_eq!(
        state,
        json!({ "online": true, "transport": "wifi", "metered": false, "battery_percent": 12,
                "charging": false, "battery_low": true, "power_save": true })
    );
}

#[quench_test::test]
fn transports_and_power_are_told_apart() {
    for (interface, transport) in [
        ("enp3s0", "ethernet"),
        ("eth0", "ethernet"),
        ("wwan0", "cellular"),
        ("tun0", "other"),
    ] {
        let dir = sys(&[
            (&format!("class/net/{interface}/operstate"), "up\n"),
            (&format!("class/net/{interface}/device/x"), ""),
        ]);
        let state = DeviceState::at(dir.path(), Duration::from_secs(60)).state();
        assert_eq!(state["transport"], json!(transport), "{interface}");
    }
    // Nothing up; no battery and the mains plugged in: charging, with no percentage.
    let dir = sys(&[
        ("class/net/wlan0/operstate", "down\n"),
        ("class/power_supply/AC/online", "1\n"),
    ]);
    let state = DeviceState::at(dir.path(), Duration::from_secs(60)).state();
    assert_eq!(state["online"], json!(false));
    assert_eq!(state["transport"], json!("none"));
    assert_eq!(
        (state["battery_percent"].clone(), state["charging"].clone()),
        (json!(-1), json!(true))
    );
    assert_eq!(state["battery_low"], json!(false));
    // A full or charging battery is charging, and not low.
    let dir = sys(&[
        ("class/power_supply/BAT1/capacity", "5\n"),
        ("class/power_supply/BAT1/status", "Charging\n"),
    ]);
    let state = DeviceState::at(dir.path(), Duration::from_secs(60)).state();
    assert_eq!(
        (state["charging"].clone(), state["battery_low"].clone()),
        (json!(true), json!(false))
    );
    // An empty tree is an offline machine on mains power with no battery.
    let none = DeviceState::at(&dir.path().join("none"), Duration::from_secs(60)).state();
    assert_eq!(none["online"], json!(false));
}

#[quench_test::test]
fn watching_pushes_a_state_event_on_change_and_stops_when_told() {
    let dir = sys(&[
        ("class/power_supply/BAT0/capacity", "80\n"),
        ("class/power_supply/BAT0/status", "Discharging\n"),
    ]);
    let page = Page::new(DeviceState::at(dir.path(), Duration::from_millis(20)));
    assert_eq!(page.call("device", "watch", &json!({}))["ok"], Value::Null);
    std::thread::sleep(Duration::from_millis(80));
    std::fs::write(dir.path().join("class/power_supply/BAT0/capacity"), "79\n").unwrap();
    let mut event = None;
    for _ in 0..200 {
        event = page
            .messages
            .lock()
            .unwrap()
            .iter()
            .find(|m| m["event"] == json!("state"))
            .cloned();
        if event.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let event = event.expect("a state event");
    assert_eq!(
        (
            event["plugin"].clone(),
            event["data"]["battery_percent"].clone()
        ),
        (json!("device"), json!(79))
    );
    // After `unwatch` nothing more arrives.
    assert_eq!(
        page.call("device", "unwatch", &json!({}))["ok"],
        Value::Null
    );
    std::thread::sleep(Duration::from_millis(60));
    let seen = page.messages.lock().unwrap().len();
    std::fs::write(dir.path().join("class/power_supply/BAT0/capacity"), "70\n").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(page.messages.lock().unwrap().len(), seen);
    assert_eq!(
        code(&page.call("device", "nope", &json!({}))),
        "unknown_method"
    );
}

#[quench_test::test]
fn secrets_go_to_the_keyring_on_stdin_and_come_back() {
    let fake = Fake::answering(vec![
        ok(""),                        // set
        ok("s3cret\n with a newline"), // get
        failed(),                      // get of a missing item
        ok(""),                        // remove
        // libsecret prints the attributes on stderr and the secrets on stdout.
        Ok(Output {
            success: true,
            stdout: "[/1]\nlabel = x\nsecret = hunter2\n".to_string(),
            stderr: "attribute.key = b\nattribute.app = my\nattribute.key = a\nattribute.key = a\n"
                .to_string(),
        }),
        ok(""), // clear
    ]);
    let page = Page::new(SecureStorage::new("my", fake.clone()));
    assert_eq!(
        page.call(
            "secure_storage",
            "set",
            &json!({ "key": "token", "value": "s3cret" })
        )["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "get", &json!({ "key": "token" }))["ok"],
        json!("s3cret\n with a newline"),
        "the value is exactly what the keyring printed"
    );
    assert_eq!(
        page.call("secure_storage", "get", &json!({ "key": "gone" }))["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "remove", &json!({ "key": "token" }))["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "keys", &json!({}))["ok"],
        json!(["a", "b"])
    );
    assert_eq!(
        page.call("secure_storage", "clear", &json!({}))["ok"],
        Value::Null
    );
    let ran = fake.ran.lock().unwrap();
    assert_eq!(
        ran[0],
        (
            "secret-tool".to_string(),
            ["store", "--label", "my token", "app", "my", "key", "token"]
                .map(String::from)
                .to_vec(),
            Some(b"s3cret".to_vec())
        )
    );
    assert!(
        ran.iter()
            .all(|(_, args, _)| !args.iter().any(|a| a == "s3cret")),
        "the secret is never on a command line"
    );
    assert_eq!(ran[1].1, ["lookup", "app", "my", "key", "token"]);
    assert_eq!(ran[3].1, ["clear", "app", "my", "key", "token"]);
    assert_eq!(ran[4].1, ["search", "--all", "app", "my"]);
    assert_eq!(ran[5].1, ["clear", "app", "my"]);
}

#[quench_test::test]
fn keys_are_read_from_either_output_and_never_include_a_secret() {
    // An older libsecret printed the attributes on standard output.
    let fake = Fake::answering(vec![ok(
        "[/org/a]\nattribute.key = b\nsecret = attribute.key = not-a-key\nattribute.key = a\n",
    )]);
    let page = Page::new(SecureStorage::new("my", fake));
    assert_eq!(
        page.call("secure_storage", "keys", &json!({}))["ok"],
        json!(["a", "b"])
    );
}

#[quench_test::test]
fn the_keyring_failing_is_told_apart_from_a_missing_secret() {
    let fake = Fake::answering(vec![
        failed(),
        Err(io::Error::new(io::ErrorKind::NotFound, "no such file")),
    ]);
    let page = Page::new(SecureStorage::new("my", fake));
    let refused = page.call(
        "secure_storage",
        "set",
        &json!({ "key": "k", "value": "v" }),
    );
    assert_eq!(code(&refused), "internal");
    let missing_tool = page.call("secure_storage", "keys", &json!({}));
    assert_eq!(code(&missing_tool), "unavailable");
    assert!(
        missing_tool["err"]["message"]
            .as_str()
            .unwrap()
            .contains("secret-tool")
    );
    assert_eq!(
        code(&page.call("secure_storage", "get", &json!({}))),
        "invalid_args"
    );
    assert_eq!(
        code(&page.call("secure_storage", "set", &json!({ "key": "k" }))),
        "invalid_args"
    );
    assert_eq!(
        code(&page.call("secure_storage", "nope", &json!({}))),
        "unknown_method"
    );
}

#[quench_test::test]
fn a_notification_is_posted_with_notify_send() {
    let fake = Fake::answering(vec![ok(""), failed()]);
    let page = Page::new(Notifications::new("Notes", fake.clone()));
    let shown = page.call(
        "notifications",
        "show",
        &json!({ "id": 7, "title": "Hi", "body": "There", "channel": "c" }),
    );
    assert_eq!(shown["ok"], json!({ "posted": true }));
    assert_eq!(
        fake.ran.lock().unwrap()[0].1,
        ["--app-name", "Notes", "--replace-id", "7", "Hi", "There"]
    );
    let refused = page.call(
        "notifications",
        "show",
        &json!({ "id": 8, "title": "Hi", "body": "There" }),
    );
    assert_eq!(code(&refused), "unavailable");
    assert_eq!(
        code(&page.call("notifications", "show", &json!({ "id": 1 }))),
        "invalid_args"
    );
    assert_eq!(
        page.call("notifications", "permission", &json!({}))["ok"],
        json!({ "granted": true })
    );
    assert_eq!(
        page.call("notifications", "channel", &json!({ "id": "c" }))["ok"],
        Value::Null
    );
    assert_eq!(
        code(&page.call("notifications", "nope", &json!({}))),
        "unknown_method"
    );
}

#[quench_test::test]
fn a_reminder_waits_fires_once_and_can_be_cancelled() {
    let fake = Fake::answering(vec![]);
    let page = Page::new(Notifications::new("Notes", fake.clone()));
    let soon = 60;
    let at = |ms: i64| {
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        now + ms
    };
    let armed = page.call(
        "notifications",
        "schedule",
        &json!({ "id": 1, "at_ms": at(soon), "title": "Soon", "body": "b" }),
    );
    assert_eq!(armed["ok"]["exact"], json!(true));
    page.call(
        "notifications",
        "schedule",
        &json!({ "id": 2, "at_ms": at(60_000), "title": "Later", "body": "b", "channel": "x" }),
    );
    page.call(
        "notifications",
        "schedule",
        &json!({ "id": 3, "at_ms": at(80), "title": "Cancelled", "body": "b" }),
    );
    page.call("notifications", "cancel", &json!({ "id": 3 }));
    let pending = page.call("notifications", "pending", &json!({}))["ok"].clone();
    let titles: Vec<_> = pending
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_string())
        .collect();
    assert!(
        titles.contains(&"Later".to_string()) && !titles.contains(&"Cancelled".to_string()),
        "{titles:?}"
    );
    std::thread::sleep(Duration::from_millis(300));
    let ran = fake.ran.lock().unwrap().clone();
    let said: Vec<_> = ran.iter().map(|r| r.1[4].clone()).collect();
    assert_eq!(
        said,
        ["Soon"],
        "the due one fired once; the cancelled and the later ones did not"
    );
    let pending = page.call("notifications", "pending", &json!({}))["ok"].clone();
    assert_eq!(
        pending.as_array().unwrap().len(),
        1,
        "a fired reminder is no longer pending"
    );
    // Replacing a reminder under its id keeps only the new one.
    page.call(
        "notifications",
        "schedule",
        &json!({ "id": 2, "at_ms": at(60), "title": "Replaced", "body": "b" }),
    );
    std::thread::sleep(Duration::from_millis(300));
    let said: Vec<_> = fake
        .ran
        .lock()
        .unwrap()
        .iter()
        .map(|r| r.1[4].clone())
        .collect();
    assert_eq!(said, ["Soon", "Replaced"]);
}

#[quench_test::test]
fn repeating_reminders_and_bad_arguments_are_refused() {
    let page = Page::new(Notifications::new("Notes", Fake::answering(vec![])));
    let repeating = page.call("notifications", "schedule", &json!({
        "id": 1, "at_ms": 1, "title": "t", "body": "b", "repeat_zone": "Europe/Warsaw", "repeat_time": "08:00" }));
    assert_eq!(code(&repeating), "unsupported");
    // A null repeat is no repeat.
    let once = page.call("notifications", "schedule", &json!({
        "id": 1, "at_ms": 4_000_000_000_000_i64, "title": "t", "body": "b", "repeat_zone": null, "repeat_time": null }));
    assert_eq!(once["ok"]["exact"], json!(true));
    assert_eq!(
        code(&page.call("notifications", "schedule", &json!({ "id": 1 }))),
        "invalid_args"
    );
}

#[quench_test::test]
fn only_files_and_web_links_are_opened() {
    let fake = Fake::answering(vec![]);
    let page = Page::new(Share::new(fake.clone()));
    assert_eq!(
        page.call(
            "share",
            "open",
            &json!({ "uri": "file:///tmp/a.pdf", "mime": "application/pdf" })
        )["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("share", "file", &json!({ "uri": "https://example.com/x" }))["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("share", "text", &json!({ "text": "a b&c", "title": "T" }))["ok"],
        Value::Null
    );
    let spawned = fake.spawned.lock().unwrap().clone();
    assert_eq!(
        spawned[0],
        (
            "xdg-open".to_string(),
            vec!["file:///tmp/a.pdf".to_string()]
        )
    );
    assert_eq!(spawned[2].1, ["mailto:?subject=T&body=a%20b%26c"]);
    for hostile in [
        "--help",
        "javascript:alert(1)",
        "/etc/passwd",
        "ftp://x",
        "",
    ] {
        assert_eq!(
            code(&page.call("share", "open", &json!({ "uri": hostile }))),
            "invalid_args",
            "{hostile:?}"
        );
    }
    assert_eq!(
        code(&page.call("share", "open", &json!({}))),
        "invalid_args"
    );
    assert_eq!(
        code(&page.call("share", "nope", &json!({}))),
        "unknown_method"
    );
    // No xdg-open at all.
    let page = Page::new(Share::new(Arc::new(Nothing)));
    assert_eq!(
        code(&page.call("share", "open", &json!({ "uri": "https://x" }))),
        "unavailable"
    );
}

#[quench_test::test]
fn the_system_runner_runs_programs_feeds_stdin_and_reports_failure() {
    let system = System;
    let echoed = system.run("cat", &[], Some(b"through stdin")).unwrap();
    assert_eq!(
        echoed,
        Output {
            success: true,
            stdout: "through stdin".to_string(),
            stderr: String::new()
        }
    );
    let loud = system
        .run("sh", &["-c", "echo out; echo err >&2"], None)
        .unwrap();
    assert_eq!(
        (loud.stdout.as_str(), loud.stderr.as_str()),
        ("out\n", "err\n")
    );
    assert!(system.run("true", &[], None).unwrap().success);
    assert!(!system.run("false", &[], None).unwrap().success);
    // A program that never reads its input is not an error of ours.
    assert!(
        system
            .run("true", &[], Some(&vec![0u8; 1 << 20]))
            .unwrap()
            .success
    );
    assert!(system.run("/no/such/program", &[], None).is_err());
    system.spawn("true", &[]).unwrap();
    assert!(system.spawn("/no/such/program", &[]).is_err());
}

/// A chooser that picks what it was told to, at once.
struct Picks(
    Mutex<Option<Vec<std::path::PathBuf>>>,
    Mutex<Vec<(bool, bool)>>,
);

impl quench_desktop::plugins::Chooser for Picks {
    fn choose(
        &self,
        folder: bool,
        multiple: bool,
        done: Box<dyn FnOnce(Option<Vec<std::path::PathBuf>>) + Send>,
    ) {
        self.1.lock().unwrap().push((folder, multiple));
        done(self.0.lock().unwrap().clone());
    }
}

fn files_page(pick: Option<Vec<std::path::PathBuf>>) -> (Page, Arc<Picks>) {
    let picks = Arc::new(Picks(Mutex::new(pick), Mutex::new(Vec::new())));
    (
        Page::new(quench_desktop::plugins::Files::new(picks.clone())),
        picks,
    )
}

#[quench_test::test]
fn only_what_the_user_picked_can_be_listed_stat_and_read() {
    use quench_desktop::plugins::{path_of, uri_of};
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap().join("pic ked");
    std::fs::create_dir_all(root.join("sub")).unwrap();
    std::fs::write(root.join("b.txt"), "hello world").unwrap();
    std::fs::write(root.join("a.PNG"), vec![7u8; 3]).unwrap();
    std::fs::write(root.join("sub/deep.json"), "{}").unwrap();
    let other = std::fs::canonicalize(dir.path()).unwrap().join("other.txt");
    std::fs::write(&other, "secret").unwrap();
    std::os::unix::fs::symlink(&other, root.join("escape.txt")).unwrap();
    let (page, picks) = files_page(Some(vec![root.clone()]));

    // Nothing is granted before the user picks.
    let before = page.call("files", "list_folder", &json!({ "uri": uri_of(&root) }));
    assert_eq!(code(&before), "permission_denied");

    let picked = page.call("files", "pick_folder", &json!({}));
    let folder = picked["ok"]["uris"][0].as_str().unwrap().to_string();
    assert_eq!(folder, uri_of(&root));
    assert!(folder.contains("pic%20ked"));
    assert_eq!(picks.1.lock().unwrap()[0], (true, false));

    let listing = page.call("files", "list_folder", &json!({ "uri": folder }))["ok"].clone();
    let names: Vec<_> = listing
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        names,
        ["sub", "a.PNG", "b.txt"],
        "folders first, then by name, and the link out is not listed"
    );
    assert_eq!(listing[0]["is_dir"], json!(true));
    assert_eq!(listing[1]["mime"], json!("image/png"));
    assert_eq!(
        (listing[2]["size"].clone(), listing[2]["mime"].clone()),
        (json!(11), json!("text/plain"))
    );
    let limited = page.call(
        "files",
        "list_folder",
        &json!({ "uri": folder, "limit": 1 }),
    )["ok"]
        .clone();
    assert_eq!(limited.as_array().unwrap().len(), 1);

    let file = uri_of(&root.join("b.txt"));
    let stat = page.call("files", "stat", &json!({ "uri": file }))["ok"].clone();
    assert_eq!(
        stat,
        json!({ "name": "b.txt", "size": 11, "mime": "text/plain" })
    );
    let stat_dir = page.call(
        "files",
        "stat",
        &json!({ "uri": uri_of(&root.join("sub")) }),
    )["ok"]
        .clone();
    assert_eq!(
        (stat_dir["size"].clone(), stat_dir["mime"].clone()),
        (json!(0), Value::Null)
    );

    let chunk = page.call(
        "files",
        "read",
        &json!({ "uri": file, "offset": 6, "length": 100 }),
    )["ok"]
        .clone();
    assert_eq!(
        chunk,
        json!({ "data": "d29ybGQ=", "size": 11, "eof": true })
    );
    let first = page.call(
        "files",
        "read",
        &json!({ "uri": file, "offset": 0, "length": 5 }),
    )["ok"]
        .clone();
    assert_eq!(
        (first["data"].clone(), first["eof"].clone()),
        (json!("aGVsbG8="), json!(false))
    );

    // A link that leaves the folder, a file beside it, and a folder's parent are not granted.
    for outside in [
        uri_of(&root.join("escape.txt")),
        uri_of(&other),
        uri_of(root.parent().unwrap()),
    ] {
        assert_eq!(
            code(&page.call("files", "read", &json!({ "uri": outside }))),
            "permission_denied",
            "{outside}"
        );
    }
    assert_eq!(path_of(&uri_of(&root)).unwrap(), root);
}

#[quench_test::test]
fn a_picked_file_is_granted_alone_and_cancelling_is_an_error() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    std::fs::write(base.join("one.txt"), "1").unwrap();
    std::fs::write(base.join("two.txt"), "2").unwrap();
    let (page, picks) = files_page(Some(vec![base.join("one.txt")]));
    let picked = page.call("files", "pick_files", &json!({ "multiple": true }));
    assert_eq!(picked["ok"]["uris"].as_array().unwrap().len(), 1);
    assert_eq!(picks.1.lock().unwrap()[0], (false, true));
    let uri = |name: &str| quench_desktop::plugins::uri_of(&base.join(name));
    assert_eq!(
        page.call("files", "stat", &json!({ "uri": uri("one.txt") }))["ok"]["size"],
        json!(1)
    );
    assert_eq!(
        code(&page.call("files", "stat", &json!({ "uri": uri("two.txt") }))),
        "permission_denied"
    );
    assert_eq!(
        code(&page.call("files", "stat", &json!({ "uri": uri("gone.txt") }))),
        "not_found"
    );
    // Cancelled, and an empty pick, both say nothing was picked.
    *picks.0.lock().unwrap() = None;
    assert_eq!(
        code(&page.call("files", "pick_folder", &json!({}))),
        "cancelled"
    );
    *picks.0.lock().unwrap() = Some(vec![]);
    assert_eq!(
        code(&page.call("files", "pick_files", &json!({}))),
        "cancelled"
    );
    assert_eq!(picks.1.lock().unwrap().last(), Some(&(false, false)));
}

#[quench_test::test]
fn bad_addresses_and_unavailable_methods_are_refused() {
    let (page, _) = files_page(None);
    for uri in [
        "https://x/y",
        "/etc/passwd",
        "file://relative",
        "file://%zz",
        "file:///a%00b",
        "",
    ] {
        assert_eq!(
            code(&page.call("files", "stat", &json!({ "uri": uri }))),
            "invalid_args",
            "{uri:?}"
        );
    }
    assert_eq!(
        code(&page.call("files", "stat", &json!({}))),
        "invalid_args"
    );
    for method in ["media", "upload", "cancel_upload"] {
        assert_eq!(
            code(&page.call("files", method, &json!({}))),
            "unsupported",
            "{method}"
        );
    }
    assert_eq!(
        code(&page.call("files", "nope", &json!({}))),
        "unknown_method"
    );
    assert_eq!(
        quench_desktop::plugins::path_of("file://localhost/tmp/a%20b").unwrap(),
        std::path::PathBuf::from("/tmp/a b")
    );
}

#[quench_test::test]
fn reads_are_chunked_and_a_folder_cannot_be_read() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    let big = vec![b'x'; quench_desktop::plugins::CHUNK + 10];
    std::fs::write(base.join("big.bin"), &big).unwrap();
    let (page, _) = files_page(Some(vec![base.clone()]));
    page.call("files", "pick_folder", &json!({}));
    let uri = quench_desktop::plugins::uri_of(&base.join("big.bin"));
    let first = page.call("files", "read", &json!({ "uri": uri }))["ok"].clone();
    assert_eq!(first["eof"], json!(false));
    let decoded = quench_core::base64::STANDARD
        .decode(first["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        decoded.len(),
        quench_desktop::plugins::CHUNK,
        "the most a call returns"
    );
    let rest = page.call(
        "files",
        "read",
        &json!({ "uri": uri, "offset": quench_desktop::plugins::CHUNK }),
    )["ok"]
        .clone();
    assert_eq!(
        (rest["eof"].clone(), rest["size"].clone()),
        (json!(true), json!(big.len()))
    );
    assert_eq!(
        code(&page.call("files", "read", &json!({ "uri": uri, "offset": -1 }))),
        "invalid_args"
    );
    let folder = quench_desktop::plugins::uri_of(&base);
    assert_eq!(
        code(&page.call("files", "read", &json!({ "uri": folder }))),
        "invalid_args"
    );
}

/// The real keyring and notification service, when asked for with `QUENCH_DESKTOP_REAL=1` (they need a session
/// with a secret service and a notification daemon, and a notification appears on the screen).
#[quench_test::test]
fn the_real_keyring_and_notification_service_work_when_there_are_any() {
    if std::env::var("QUENCH_DESKTOP_REAL").as_deref() != Ok("1") {
        eprintln!("QUENCH_DESKTOP_REAL is not 1: the real keyring and notifications are not tried");
        return;
    }
    let run: Arc<dyn Runner> = Arc::new(System);
    let app = format!("quench-desktop-test-{}", std::process::id());
    let page = Page::new(SecureStorage::new(&app, Arc::clone(&run)));
    let secret = "pässword with\nnewline and 'quotes' \"x\"";
    assert_eq!(
        page.call(
            "secure_storage",
            "set",
            &json!({ "key": "token", "value": secret })
        )["ok"],
        Value::Null
    );
    assert_eq!(
        page.call(
            "secure_storage",
            "set",
            &json!({ "key": "other", "value": "2" })
        )["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "get", &json!({ "key": "token" }))["ok"],
        json!(secret)
    );
    assert_eq!(
        page.call("secure_storage", "keys", &json!({}))["ok"],
        json!(["other", "token"])
    );
    assert_eq!(
        page.call("secure_storage", "remove", &json!({ "key": "token" }))["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "get", &json!({ "key": "token" }))["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "clear", &json!({}))["ok"],
        Value::Null
    );
    assert_eq!(
        page.call("secure_storage", "keys", &json!({}))["ok"],
        json!([])
    );

    let page = Page::new(Notifications::new("quench desktop test", run));
    let shown = page.call("notifications", "show", &json!({ "id": 1, "title": "quench-desktop", "body": "the real notification service answered" }));
    assert_eq!(shown["ok"], json!({ "posted": true }));
}

#[quench_test::test]
fn a_named_pipe_in_a_picked_folder_cannot_hang_a_call() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let base = std::fs::canonicalize(dir.path()).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(base.join("pipe"))
            .status()
            .unwrap()
            .success()
    );
    let (page, _) = files_page(Some(vec![base.clone()]));
    page.call("files", "pick_folder", &json!({}));
    let uri = quench_desktop::plugins::uri_of(&base.join("pipe"));
    // Opened without blocking, so nobody has to write to it for these to answer.
    let stat = page.call("files", "stat", &json!({ "uri": uri }))["ok"].clone();
    assert_eq!(stat["name"], json!("pipe"));
    let read = page.call("files", "read", &json!({ "uri": uri }));
    assert_eq!(read["ok"]["data"], json!(""), "{read}");
    let listing = page.call(
        "files",
        "list_folder",
        &json!({ "uri": quench_desktop::plugins::uri_of(&base) }),
    )["ok"]
        .clone();
    assert_eq!(listing.as_array().unwrap().len(), 1);
}
