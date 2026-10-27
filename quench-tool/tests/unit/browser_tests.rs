use super::support::*;
use quench_tool::browser::run;

fn ready(label: &str) -> (Fake, quench_tool::project::Project) {
    let p = project(label, "fullstack");
    std::fs::create_dir_all(p.root.join("tests")).unwrap();
    std::fs::write(p.root.join("tests/browser.mjs"), "").unwrap();
    let mut sys = Fake::default();
    sys.existing.insert(p.root.join("tests/browser.mjs"));
    sys.programs
        .extend(["node".to_string(), "chromium".to_string()]);
    sys.listening.set(true);
    (sys, p)
}

#[quench_test::test]
fn it_builds_serves_on_a_free_port_runs_the_script_and_stops_the_server() {
    let (sys, p) = ready("browser");
    let mut out = Vec::new();
    run(&sys, &mut out, &p).unwrap();
    let lines = sys.lines();
    assert_eq!(
        lines[0],
        "cargo build --target wasm32-unknown-unknown --profile wasm-dev -p demo-app"
    );
    assert_eq!(lines[1], "cargo build -p demo-server");
    assert_eq!(lines[2], "node tests/browser.mjs http://127.0.0.1:4242");
    let ran = sys.ran.borrow();
    assert!(
        ran[2]
            .env
            .contains(&("CHROMIUM".to_string(), "chromium".to_string()))
    );
    let spawned = sys.spawned.borrow();
    let env: std::collections::HashMap<_, _> = spawned[0].0.env.iter().cloned().collect();
    assert_eq!(env["PORT"], "4242");
    assert!(
        !env.contains_key("QUENCH_DEV"),
        "the page under test has no reload script"
    );
    assert!(spawned[0].1.get(), "the server is stopped");
}

#[quench_test::test]
fn a_failing_script_fails_the_run_and_still_stops_the_server() {
    let (sys, p) = ready("browser-fail");
    *sys.fail_on.borrow_mut() = Some("node".to_string());
    let e = run(&sys, &mut Vec::new(), &p).unwrap_err().to_string();
    assert!(e.contains("node tests/browser.mjs"), "{e}");
    assert!(sys.spawned.borrow()[0].1.get());
}

#[quench_test::test]
fn a_server_that_never_listens_is_reported_and_stopped() {
    let (sys, p) = ready("browser-silent");
    sys.listening.set(false);
    let e = run(&sys, &mut Vec::new(), &p).unwrap_err().to_string();
    assert!(e.contains("did not start listening on port 4242"), "{e}");
    assert!(sys.spawned.borrow()[0].1.get());
    assert!(sys.lines().iter().all(|l| !l.starts_with("node")));
}

#[quench_test::test]
fn missing_tools_are_named_before_anything_is_built() {
    let (mut sys, p) = ready("browser-tools");
    sys.programs.remove("node");
    assert!(
        run(&sys, &mut Vec::new(), &p)
            .unwrap_err()
            .to_string()
            .contains("node")
    );
    sys.programs.insert("node".into());
    sys.programs.remove("chromium");
    assert!(
        run(&sys, &mut Vec::new(), &p)
            .unwrap_err()
            .to_string()
            .contains("Chromium")
    );
    sys.programs.insert("google-chrome".into());
    run(&sys, &mut Vec::new(), &p).unwrap();
    assert!(sys.ran.borrow().iter().any(|c| {
        c.env
            .contains(&("CHROMIUM".to_string(), "google-chrome".to_string()))
    }));
    let (mut bare, q) = ready("browser-nofile");
    bare.existing.clear();
    assert!(
        run(&bare, &mut Vec::new(), &q)
            .unwrap_err()
            .to_string()
            .contains("tests/browser.mjs")
    );
    assert!(bare.ran.borrow().is_empty());
}
