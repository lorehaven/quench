//! The security gate, one case per stage, so the run's `quench-test` report is the security report.
//!
//! Needs `--features gate`, cargo-deny and cargo-audit, and the network (the advisory database).
//! Run with `--test-threads=1`: the attack-suite cases are nested `cargo test` runs.
//!
//! The nested runs inherit `QUENCH_TEST_REPORT_DIR`, so each attack test shows up in the same report
//! as its own case, next to the stage that ran it.

#![allow(clippy::wildcard_imports)]

use quench_security::*;

fn root() -> std::path::PathBuf {
    workspace_root()
}

fn ensure(ran: &Ran, what: &str) {
    assert!(ran.success, "{what} failed:\n{}", ran.tail(40));
}

// --- supply chain: re-checked nightly because advisories appear without any code changing ---

#[quench_test::test]
fn supply_chain_cargo_deny() {
    ensure(
        &run(&root(), "cargo", &["deny", "check"], &[]),
        "cargo deny check",
    );
}

#[quench_test::test]
fn supply_chain_cargo_audit() {
    ensure(
        &run(&root(), "cargo", &["audit", "--deny", "warnings"], &[]),
        "cargo audit (RustSec advisories vs Cargo.lock)",
    );
}

// --- static ---

#[quench_test::test]
fn static_unsafe_is_confined_to_allowlisted_files() {
    let root = root();
    let allowed = allowlist(
        &std::fs::read_to_string(root.join("scripts/unsafe-allowlist.txt"))
            .expect("scripts/unsafe-allowlist.txt must exist"),
    );
    let found = files_using_unsafe(&root);
    let new: Vec<_> = found.iter().filter(|f| !allowed.contains(f)).collect();
    assert!(
        new.is_empty(),
        "NEW unsafe, not reviewed (add to scripts/unsafe-allowlist.txt once justified): {new:?}"
    );
    let stale: Vec<_> = allowed.iter().filter(|f| !found.contains(f)).collect();
    assert!(
        stale.is_empty(),
        "stale allowlist entries (no longer contain unsafe): {stale:?}"
    );
    // A scan that found nothing at all is a broken scan, not a clean workspace.
    assert!(
        !rust_files(&root).is_empty(),
        "the scan saw no Rust files under {}",
        root.display()
    );
}

#[quench_test::test]
fn static_no_secrets_in_sources() {
    let root = root();
    assert!(
        !text_files(&root).is_empty(),
        "the scan saw no files under {}",
        root.display()
    );
    // Reviewed exceptions, one path per line - each a conscious decision that the file is not a secret.
    let allowed = std::fs::read_to_string(root.join("scripts/secrets-allowlist.txt"))
        .map(|text| allowlist(&text))
        .unwrap_or_default();
    let found: Vec<_> = secrets_in(&root)
        .into_iter()
        .filter(|(file, _, _)| {
            !allowed
                .iter()
                .any(|a| a.trim_start_matches("./") == file.trim_start_matches("./"))
        })
        .collect();
    assert!(
        found.is_empty(),
        "secrets in sources (allow a reviewed false positive in scripts/secrets-allowlist.txt): {found:?}"
    );
}

#[quench_test::test]
fn static_no_symlink_points_outside_the_checkout() {
    // A symlink to an absolute path works on one machine and breaks every clone made anywhere else.
    let found = symlinks_leaving_the_project(&root());
    assert!(
        found.is_empty(),
        "symlinks that leave the project (path, target): {found:?}"
    );
}

// --- attack suites (nested runs; their individual cases land in the same report) ---

fn nested(args: &[&str], what: &str) {
    let mut full = vec!["test"];
    full.extend_from_slice(args);
    full.extend_from_slice(&["--", "--test-threads=1"]);
    ensure(&run(&root(), "cargo", &full, &[]), what);
}

#[quench_test::test]
fn attack_http_smuggling_and_resource_exhaustion() {
    nested(
        &[
            "-p",
            "quench-proto",
            "--features",
            "server,client",
            "--test",
            "security_h1",
        ],
        "wire-level HTTP attacks",
    );
}

#[quench_test::test]
fn attack_parser_properties() {
    let iterations = std::env::var("QUENCH_FUZZ_ITERS").unwrap_or_else(|_| "200000".to_string());
    let mut full = vec![
        "test",
        "-p",
        "quench-proto",
        "--features",
        "server,client",
        "--test",
        "fuzz_parsers",
        "--",
        "--test-threads=1",
    ];
    full.shrink_to_fit();
    ensure(
        &run(
            &root(),
            "cargo",
            &full,
            &[("QUENCH_FUZZ_ITERS", &iterations)],
        ),
        "mutation-fuzzed parser properties",
    );
}

#[quench_test::test]
fn attack_jwt_verification() {
    nested(
        &["-p", "quench-crypto", "--test", "security_jwt"],
        "JWT attacks",
    );
}

#[quench_test::test]
fn attack_framework_layer() {
    nested(
        &["-p", "quench-http", "--test", "security_http"],
        "framework-layer attacks",
    );
}

quench_test::main!();
