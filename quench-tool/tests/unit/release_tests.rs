use super::support::*;
use quench_tool::release::{build, bundle, wasm_profile};

fn put(path: &std::path::Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[quench_test::test]
fn the_profile_follows_release() {
    assert_eq!(wasm_profile(true), "wasm");
    assert_eq!(wasm_profile(false), "wasm-dev");
}

#[quench_test::test]
fn a_plain_build_makes_the_module_and_the_server() {
    let p = project("plain", "web");
    let sys = Fake::default();
    let mut out = Vec::new();
    build(&sys, &mut out, &p, false, true).unwrap();
    assert_eq!(
        sys.lines(),
        [
            "cargo build --target wasm32-unknown-unknown --profile wasm-dev -p demo-app",
            "cargo build -p demo-server",
        ]
    );
    assert!(text(out).contains("built "));
}

#[quench_test::test]
fn a_release_build_collects_dist_and_embeds_the_module() {
    let p = project("release", "web");
    put(&p.wasm_path("wasm"), b"\0asm-module");
    put(&p.server_path("release"), b"binary");
    let sys = Fake::default();
    let mut out = Vec::new();
    build(&sys, &mut out, &p, true, true).unwrap();
    assert_eq!(
        sys.lines(),
        [
            "cargo build --target wasm32-unknown-unknown --profile wasm -p demo-app",
            "cargo build -p demo-server --release --features embed",
        ]
    );
    assert_eq!(
        std::fs::read(p.dist().join("app.wasm")).unwrap(),
        b"\0asm-module"
    );
    assert_eq!(
        std::fs::read(p.dist().join("demo-server")).unwrap(),
        b"binary"
    );
    let said = text(out);
    assert!(
        said.contains("wasm-opt not found") && said.contains("module built in"),
        "{said}"
    );
}

#[quench_test::test]
fn no_embed_leaves_the_module_a_file_and_wasm_opt_is_used_when_there() {
    let p = project("no-embed", "web");
    put(&p.wasm_path("wasm"), b"m");
    put(&p.server_path("release"), b"b");
    let mut sys = Fake::default();
    sys.programs.insert("wasm-opt".into());
    let mut out = Vec::new();
    build(&sys, &mut out, &p, true, false).unwrap();
    let lines = sys.lines();
    assert!(
        lines[1].starts_with("wasm-opt -Oz -o ") && lines[1].contains("app.wasm"),
        "{lines:?}"
    );
    assert_eq!(lines[2], "cargo build -p demo-server --release");
    assert!(text(out).contains("reads the module"));
}

#[quench_test::test]
fn a_failing_step_stops_the_build_and_names_the_command() {
    let p = project("fails", "web");
    let sys = Fake::default();
    *sys.fail_on.borrow_mut() = Some("demo-server".to_string());
    let e = build(&sys, &mut Vec::new(), &p, false, true)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("cargo build -p demo-server") && e.contains("101"),
        "{e}"
    );
}

#[quench_test::test]
fn missing_build_outputs_are_reported() {
    let p = project("no-output", "web");
    let e = build(&Fake::default(), &mut Vec::new(), &p, true, true)
        .unwrap_err()
        .to_string();
    assert!(e.contains("demo_app.wasm"), "{e}");
    put(&p.wasm_path("wasm"), b"m");
    let e = build(&Fake::default(), &mut Vec::new(), &p, true, true)
        .unwrap_err()
        .to_string();
    assert!(e.contains("demo-server"), "{e}");
}

#[quench_test::test]
fn a_program_that_cannot_start_is_named() {
    use quench_tool::system::Cmd;
    let sys = Fake::default();
    let cmd = Cmd::new("missing-program", &[], std::path::Path::new("."));
    let e = quench_tool::release::step(&sys, &mut Vec::new(), &cmd)
        .unwrap_err()
        .to_string();
    assert!(e.contains("cannot run `missing-program`"), "{e}");
}

#[quench_test::test]
fn a_bundle_is_written_by_the_projects_own_server() {
    let p = project("bundle", "web");
    let stale = p.dist().join("www/_quench/old.deadbeef.css");
    put(&stale, b"x");
    let sys = Fake::default();
    let mut out = Vec::new();
    bundle(&sys, &mut out, &p).unwrap();
    let lines = sys.lines();
    assert_eq!(
        lines[0],
        "cargo build --target wasm32-unknown-unknown --profile wasm -p demo-app"
    );
    assert_eq!(lines[1], "cargo build -p demo-server");
    assert!(lines[2].ends_with("debug/demo-server"), "{lines:?}");
    let ran = sys.ran.borrow();
    let env: std::collections::HashMap<_, _> = ran[2].env.iter().cloned().collect();
    assert!(env["QUENCH_EXPORT"].ends_with("dist/www"));
    assert!(
        env["WASM"].ends_with("wasm/demo_app.wasm"),
        "the release module goes in the bundle"
    );
    assert!(
        !stale.exists(),
        "files of the last bundle are named by hash and must not pile up"
    );
    assert!(text(out).contains("bundle "));
}

#[quench_test::test]
fn a_failing_export_fails_the_bundle() {
    let p = project("bundle-fail", "web");
    let sys = Fake::default();
    *sys.fail_on.borrow_mut() = Some("debug/demo-server".to_string());
    let e = bundle(&sys, &mut Vec::new(), &p).unwrap_err().to_string();
    assert!(e.contains("demo-server"), "{e}");
}
