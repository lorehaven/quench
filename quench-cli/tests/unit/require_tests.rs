//! Unit tests for `require.rs`.

use quench_cli::require::require_binary;

#[test]
fn a_binary_that_is_on_path_resolves_ok() {
    assert!(require_binary("sh", "needed for shelling out").is_ok());
}

#[test]
fn a_missing_binary_reports_its_name_and_hint_in_both_display_and_debug() {
    let err = require_binary(
        "definitely-not-a-real-binary-xyz",
        "install it from somewhere",
    )
    .expect_err("bogus binary name should not resolve");

    let display = format!("{err}");
    assert!(display.contains("definitely-not-a-real-binary-xyz"));
    assert!(display.contains("not found on PATH"));
    assert!(display.contains("install it from somewhere"));

    // `Debug` is implemented in terms of `Display` on purpose, so `main`'s
    // `{:?}` error path prints the same one-line message.
    let debug = format!("{err:?}");
    assert_eq!(debug, display);
}
