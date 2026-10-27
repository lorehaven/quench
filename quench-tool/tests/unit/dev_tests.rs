use super::support::*;
use quench_tool::dev::dev;
use quench_tool::watch::{Event, Watch};

struct Script(Vec<Event>);

impl Watch for Script {
    fn poll(&mut self) -> Event {
        if self.0.is_empty() {
            Event::Stop
        } else {
            self.0.remove(0)
        }
    }
}

#[quench_test::test]
fn it_builds_starts_the_server_in_dev_mode_and_stops_it_at_the_end() {
    let p = project("dev", "web");
    let sys = Fake::default();
    let mut out = Vec::new();
    dev(
        &sys,
        &mut out,
        &p,
        3456,
        false,
        &mut Script(vec![Event::Quiet]),
    )
    .unwrap();
    assert_eq!(
        sys.lines(),
        [
            "cargo build --target wasm32-unknown-unknown --profile wasm-dev -p demo-app",
            "cargo build -p demo-server",
        ]
    );
    let spawned = sys.spawned.borrow();
    let (cmd, killed) = &spawned[0];
    assert!(
        cmd.program.ends_with("debug/demo-server"),
        "{}",
        cmd.program
    );
    let env: std::collections::HashMap<_, _> = cmd.env.iter().cloned().collect();
    assert_eq!(env["PORT"], "3456");
    assert_eq!(env["QUENCH_DEV"], "1");
    assert!(env["WASM"].ends_with("wasm-dev/demo_app.wasm"));
    assert!(killed.get());
    assert!(text(out).contains("serving http://127.0.0.1:3456"));
}

#[quench_test::test]
fn a_change_rebuilds_and_restarts() {
    let p = project("dev-change", "web");
    let sys = Fake::default();
    let mut out = Vec::new();
    dev(
        &sys,
        &mut out,
        &p,
        3000,
        false,
        &mut Script(vec![Event::Quiet, Event::Changed, Event::Quiet]),
    )
    .unwrap();
    assert_eq!(sys.lines().len(), 4, "two rounds of two builds");
    let spawned = sys.spawned.borrow();
    assert_eq!(spawned.len(), 2);
    assert!(
        spawned[0].1.get() && spawned[1].1.get(),
        "each server is stopped"
    );
    assert!(text(out).contains("change detected"));
}

#[quench_test::test]
fn a_failed_build_waits_for_the_next_change() {
    let p = project("dev-fail", "web");
    let sys = Fake::default();
    *sys.fail_on.borrow_mut() = Some("-p demo-app".to_string());
    let mut out = Vec::new();
    dev(
        &sys,
        &mut out,
        &p,
        3000,
        false,
        &mut Script(vec![Event::Quiet, Event::Changed]),
    )
    .unwrap();
    assert!(sys.spawned.borrow().is_empty(), "nothing to serve");
    let said = text(out);
    assert!(
        said.contains("error:") && said.contains("waiting for a change"),
        "{said}"
    );
}

#[quench_test::test]
fn a_server_that_exits_is_reported_once() {
    let p = project("dev-exit", "web");
    let sys = Fake::default();
    sys.exit_spawned.set(Some(1));
    let mut out = Vec::new();
    dev(
        &sys,
        &mut out,
        &p,
        3000,
        false,
        &mut Script(vec![Event::Quiet, Event::Quiet, Event::Quiet]),
    )
    .unwrap();
    assert_eq!(text(out).matches("the server stopped (exit 1)").count(), 1);
}

#[quench_test::test]
fn a_server_that_cannot_start_is_an_error_to_wait_out() {
    let p = project_named("dev-unstartable", "unstartable", "web");
    let sys = Fake::default();
    let mut out = Vec::new();
    dev(&sys, &mut out, &p, 3000, false, &mut Script(vec![])).unwrap();
    assert!(text(out).contains("cannot start"));
}

fn dev_open(sys: &Fake, p: &quench_tool::project::Project, events: Vec<Event>) -> String {
    let mut out = Vec::new();
    dev(sys, &mut out, p, 3000, true, &mut Script(events)).unwrap();
    text(out)
}

#[quench_test::test]
fn open_asks_the_desktop_to_open_the_page_once() {
    let p = project("dev-open", "web");
    let mut sys = Fake::default();
    sys.programs.insert("xdg-open".into());
    dev_open(&sys, &p, vec![Event::Changed, Event::Quiet]);
    let opens: Vec<String> = sys
        .lines()
        .into_iter()
        .filter(|l| l.starts_with("xdg-open"))
        .collect();
    assert_eq!(
        opens,
        ["xdg-open http://127.0.0.1:3000"],
        "a restart reloads the open page instead"
    );
    assert_eq!(sys.spawned.borrow().len(), 2);
}

#[quench_test::test]
fn open_falls_back_to_the_macos_command_and_says_when_there_is_none() {
    let p = project("dev-open-mac", "web");
    let mut sys = Fake::default();
    sys.programs.insert("open".into());
    dev_open(&sys, &p, vec![]);
    assert!(
        sys.lines()
            .contains(&"open http://127.0.0.1:3000".to_string())
    );
    let said = dev_open(&Fake::default(), &p, vec![]);
    assert!(
        said.contains("open http://127.0.0.1:3000 yourself"),
        "{said}"
    );
}

#[quench_test::test]
fn a_failing_opener_is_reported_not_fatal() {
    let p = project("dev-open-fail", "web");
    let mut sys = Fake::default();
    sys.programs.insert("xdg-open".into());
    *sys.fail_on.borrow_mut() = Some("xdg-open".to_string());
    let said = dev_open(&sys, &p, vec![]);
    assert!(said.contains("xdg-open failed"), "{said}");
    assert_eq!(sys.spawned.borrow().len(), 1, "the server still runs");
}

#[quench_test::test]
fn a_build_that_failed_opens_nothing_until_it_works() {
    let p = project("dev-open-late", "web");
    let mut sys = Fake::default();
    sys.programs.insert("xdg-open".into());
    *sys.fail_on.borrow_mut() = Some("-p demo-app".to_string());
    dev_open(&sys, &p, vec![Event::Changed]);
    assert!(sys.lines().iter().all(|l| !l.starts_with("xdg-open")));
}
