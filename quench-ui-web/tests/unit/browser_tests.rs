//! What the page tells the module: the wall clock, the time zone and what it kept.

use quench_ui_web::mock;

#[quench_test::test]
fn the_page_tells_the_module_its_zone_and_what_it_kept() {
    mock::reset();
    assert_eq!(
        quench_ui_web::browser::time_zone(),
        "UTC",
        "before the page says"
    );
    let (address, len) =
        mock::alloc_copy(br#"{"time_zone":"Europe/Warsaw","storage":{"session":"abc"}}"#);
    quench_ui_web::q_set_env(address, len);
    assert_eq!(quench_ui_web::browser::time_zone(), "Europe/Warsaw");
    assert_eq!(
        quench_ui_web::browser::storage_get("session").as_deref(),
        Some("abc")
    );
    assert_eq!(quench_ui_web::browser::storage_get("other"), None);
}

#[quench_test::test]
fn what_the_module_keeps_goes_to_the_page_and_is_forgotten_with_none() {
    mock::reset();
    quench_ui_web::browser::storage_set("session", Some("one"));
    assert_eq!(
        quench_ui_web::browser::storage_get("session").as_deref(),
        Some("one")
    );
    quench_ui_web::browser::storage_set("session", None);
    assert_eq!(quench_ui_web::browser::storage_get("session"), None);
    assert_eq!(
        mock::storage_sets(),
        [
            ("session".to_string(), "one".to_string()),
            ("session".to_string(), String::new())
        ]
    );
}

#[quench_test::test]
fn the_wall_clock_is_the_browsers() {
    mock::reset();
    mock::set_wall_ms(1_800_000_000_123);
    assert_eq!(quench_ui_web::browser::wall_ms(), 1_800_000_000_123);
}
