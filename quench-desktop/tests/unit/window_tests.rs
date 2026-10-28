//! The window itself, through the fixture binary. They need a display; without one they say so and pass.
use std::process::Command;

fn fixture() -> Option<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_quench-desktop-fixture"))
        .output()
        .unwrap();
    if out.status.code() == Some(77) {
        eprintln!("no display: the window tests are skipped");
        return None;
    }
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[quench_test::test]
fn a_window_serves_its_scheme_and_round_trips_messages() {
    let Some(said) = fixture() else { return };
    let lines: Vec<_> = said.lines().collect();
    assert_eq!(lines[0], "message \"ping\" at quench://app/");
    assert_eq!(lines[1], "message \"got:pong\" at quench://app/");
    // The asset came from the scheme, on the app's origin, in a secure context, with the host's user agent.
    assert_eq!(
        lines[2],
        "message \"asset:from the scheme:quench://app:true:true\" at quench://app/"
    );
    assert_eq!(lines[3], "closed");
}

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

/// The app fixture, with its control socket connected; the process is killed when this is dropped.
struct Running {
    child: std::process::Child,
    stream: UnixStream,
    lines: BufReader<UnixStream>,
}

impl Drop for Running {
    fn drop(&mut self) {
        // Ask it to end as a program does (its coverage is written then); kill it if it will not.
        let _ = writeln!(self.stream, "(call('quit', 'now', {{}}), 1)");
        for _ in 0..100 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Running {
    fn start(label: &str) -> Option<Self> {
        Self::start_with(label, None)
    }

    /// With `bin` as the only directory on the fixture's `PATH`: a program of its own, to see what it is run with.
    fn start_with(label: &str, bin: Option<&std::path::Path>) -> Option<Self> {
        let dir = quench_core::tmp::TempDir::new().unwrap();
        let socket = dir.path().join(format!("{label}.sock"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_quench-desktop-fixture"))
            .env("FIXTURE", "app")
            .env("QUENCH_DESKTOP_CONTROL", &socket)
            .envs(bin.map(|bin| ("PATH", bin)))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        let stream = loop {
            if let Ok(stream) = UnixStream::connect(&socket) {
                break stream;
            }
            if let Some(status) = child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(77), "the fixture stopped: {status}");
                eprintln!("no display: the window tests are skipped");
                return None;
            }
            assert!(Instant::now() < deadline, "no control socket");
            std::thread::sleep(Duration::from_millis(50));
        };
        // Keep the directory (and so the socket's name) for as long as the test needs it.
        std::mem::forget(dir);
        let lines = BufReader::new(stream.try_clone().unwrap());
        Some(Self {
            child,
            stream,
            lines,
        })
    }

    /// Runs a script in the page; its value as JSON text, or the error.
    fn eval(&mut self, script: &str) -> Result<String, String> {
        writeln!(self.stream, "{script}").unwrap();
        let mut line = String::new();
        self.lines.read_line(&mut line).unwrap();
        let reply: quench_serde::Value = quench_serde::json::from_str(&line).unwrap();
        reply.get("ok").map_or_else(
            || Err(reply["error"].as_str().unwrap().to_string()),
            |value| Ok(value.to_string()),
        )
    }

    /// Evaluates until the value is `want` (a page's work is not instant).
    fn eventually(&mut self, script: &str, want: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let got = self.eval(script).unwrap_or_default();
            if got == want || Instant::now() > deadline {
                return got;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[quench_test::test]
fn the_host_serves_the_bundle_and_answers_plugin_calls() {
    let Some(mut app) = Running::start("serve") else {
        return;
    };
    assert_eq!(
        app.eventually("location.origin", "\"quench://app\""),
        "\"quench://app\""
    );
    assert_eq!(app.eval("document.title").unwrap(), "\"app\"");
    assert_eq!(app.eval("isSecureContext").unwrap(), "true");
    assert_eq!(
        app.eval("navigator.userAgent.includes('QuenchHost/1')")
            .unwrap(),
        "true"
    );
    // A route of the app gets the page; a missing file is a 404 the page can see. (A promise is not a value
    // the control socket can return, so the scripts start the fetch and return 1.)
    app.eval("(fetch('/some/route').then(r => { window.s = r.status; }), 1)")
        .unwrap();
    assert_eq!(app.eventually("window.s", "200"), "200");
    app.eval("(fetch('/missing.js').then(r => { window.m = r.status; }), 1)")
        .unwrap();
    assert_eq!(app.eventually("window.m", "404"), "404");

    // The bridge: a call goes to the plugin and its answer comes back with the id.
    app.eval("(call('echo', 'say', { text: 'hi' }), 1)")
        .unwrap();
    assert_eq!(
        app.eventually(
            "JSON.stringify(answers[0])",
            "\"{\\\"id\\\":1,\\\"ok\\\":\\\"hi\\\"}\""
        ),
        "\"{\\\"id\\\":1,\\\"ok\\\":\\\"hi\\\"}\""
    );
    app.eval("(call('echo', 'fail', { code: 'teapot' }), 1)")
        .unwrap();
    assert_eq!(
        app.eventually("answers[1] && answers[1].err.code", "\"teapot\""),
        "\"teapot\""
    );
    app.eval("(call('nobody', 'x', {}), 1)").unwrap();
    assert_eq!(
        app.eventually("answers[2] && answers[2].err.code", "\"unknown_plugin\""),
        "\"unknown_plugin\""
    );
    assert_eq!(app.eval("answers.length").unwrap(), "3");
}

#[quench_test::test]
fn a_script_that_throws_is_an_error_not_a_hang() {
    let Some(mut app) = Running::start("throws") else {
        return;
    };
    app.eventually("1", "1");
    let error = app.eval("throw new Error('boom')").unwrap_err();
    assert!(error.contains("boom"), "{error}");
    assert_eq!(app.eval("undefined").unwrap(), "null");
    assert_eq!(app.eval("({ a: [1, 'x'] })").unwrap(), "{\"a\":[1,\"x\"]}");
}

#[quench_test::test]
fn links_out_of_the_app_go_to_the_system_and_the_window_stays() {
    use std::os::unix::fs::PermissionsExt;
    let dir = quench_core::tmp::TempDir::new().unwrap();
    let log = dir.path().join("opened.txt");
    // A stand-in for xdg-open that writes what it was asked to open.
    let script = dir.path().join("xdg-open");
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"$1\" >> {}\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let Some(mut app) = Running::start_with("links", Some(dir.path())) else {
        return;
    };
    app.eventually("location.origin", "\"quench://app\"");
    // A link, a script's navigation, and a new window: the web ones go to the system, the rest nowhere.
    app.eval("(window.open('https://example.com/new'), 1)")
        .unwrap();
    app.eval("(location.href = 'https://example.com/nav', 1)")
        .unwrap();
    app.eval("(location.href = 'mailto:a@b.c', 1)").unwrap();
    app.eval("(location.href = 'file:///etc/passwd', 1)")
        .unwrap();
    app.eval("(window.open('quench://app/inside'), 1)").unwrap();
    let mut opened = String::new();
    for _ in 0..100 {
        opened = std::fs::read_to_string(&log).unwrap_or_default();
        if opened.lines().count() >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut lines: Vec<_> = opened.lines().collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        [
            "https://example.com/nav",
            "https://example.com/new",
            "mailto:a@b.c"
        ],
        "{opened}"
    );
    assert_eq!(
        app.eval("location.href").unwrap(),
        "\"quench://app/\"",
        "the app did not move"
    );
}

#[quench_test::test]
fn the_standard_plugins_answer_through_the_real_bridge() {
    let Some(mut app) = Running::start("standard") else {
        return;
    };
    app.eventually("location.origin", "\"quench://app\"");
    app.eval("(call('device', 'state', {}), 1)").unwrap();
    assert_eq!(
        app.eventually(
            "answers[0] && Object.keys(answers[0].ok).sort().join()",
            "\"battery_low,battery_percent,charging,metered,online,power_save,transport\""
        ),
        "\"battery_low,battery_percent,charging,metered,online,power_save,transport\""
    );
    app.eval("(call('notifications', 'permission', {}), 1)")
        .unwrap();
    assert_eq!(
        app.eventually("answers[1] && answers[1].ok.granted", "true"),
        "true"
    );
    // `files` refuses what nobody picked, as an error with a code.
    app.eval("(call('files', 'stat', { uri: 'file:///etc/hostname' }), 1)")
        .unwrap();
    assert_eq!(
        app.eventually("answers[2] && answers[2].err.code", "\"permission_denied\""),
        "\"permission_denied\""
    );
}
