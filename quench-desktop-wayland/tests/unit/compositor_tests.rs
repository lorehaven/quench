//! The fixture window on the real compositor, when there is one (it skips, saying so, when there is not).
use std::process::Command;

#[quench_test::test]
fn a_window_opens_draws_and_closes_on_the_real_compositor() {
    let out = Command::new(env!("CARGO_BIN_EXE_quench-desktop-wayland-fixture"))
        .env("FIXTURE_SECONDS", "1")
        .output()
        .unwrap();
    if out.status.code() == Some(77) {
        eprintln!("no compositor: the real-compositor test is skipped");
        return;
    }
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{said}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(said.starts_with("opened ("), "{said}");
    assert!(said.contains("frame") && said.contains("done"), "{said}");
}
