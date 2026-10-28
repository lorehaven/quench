use quench_desktop::{App, Bundle, RunError, is_app_origin, opens_outside, origin_of};

#[quench_test::test]
fn origins_are_scheme_host_and_port() {
    assert_eq!(origin_of("quench://app/index.html?x#y"), "quench://app");
    assert_eq!(
        origin_of("http://127.0.0.1:3000/a"),
        "http://127.0.0.1:3000"
    );
    assert_eq!(origin_of("https://example.com"), "https://example.com");
    assert_eq!(origin_of("about:blank"), "");
    assert_eq!(origin_of(""), "");
}

#[quench_test::test]
fn only_the_bundle_and_the_dev_server_are_the_apps_own() {
    assert!(is_app_origin("quench://app/", None));
    assert!(is_app_origin("quench://app/users/7?x=1", None));
    assert!(!is_app_origin("quench://other/", None));
    assert!(!is_app_origin("https://app/", None));
    assert!(
        !is_app_origin("http://127.0.0.1:3000/", None),
        "the dev server only when asked for"
    );
    let dev = Some("http://127.0.0.1:3000/");
    assert!(is_app_origin("http://127.0.0.1:3000/gallery", dev));
    assert!(!is_app_origin("http://127.0.0.1:3001/", dev));
    assert!(!is_app_origin("http://localhost:3000/", dev));
    assert!(is_app_origin("quench://app/", dev));
}

#[quench_test::test]
fn links_for_the_system_are_web_and_mail() {
    for uri in [
        "https://example.com",
        "http://example.com/a",
        "mailto:a@b.c",
    ] {
        assert!(opens_outside(uri), "{uri}");
    }
    for uri in [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "quench://app/",
        "ftp://x",
        "-flag",
    ] {
        assert!(!opens_outside(uri), "{uri}");
    }
}

#[quench_test::test]
fn an_app_is_described_and_built_up() {
    let app = App::new("Notes", Bundle::new().with("index.html", &b"x"[..]));
    assert_eq!(
        (app.title.as_str(), app.width, app.height),
        ("Notes", 1000, 700)
    );
    assert!(!app.debug && app.dev_url.is_none() && app.plugins.is_empty());
    let app = app.dev("http://127.0.0.1:3000");
    assert!(app.debug);
    assert_eq!(app.dev_url.as_deref(), Some("http://127.0.0.1:3000"));
    assert!(format!("{app:?}").contains("Notes"));
    assert_eq!(
        RunError::NoDisplay.to_string(),
        "there is no display to open a window on"
    );
}
