use quench_desktop::{Bundle, content_type};

fn bundle() -> Bundle {
    Bundle::new()
        .with("index.html", &b"<html>app</html>"[..])
        .with("_quench/app.1.wasm", &b"\0asm"[..])
        .with("/style.css", &b"body{}"[..])
}

#[quench_test::test]
fn the_root_and_routes_get_the_page() {
    let b = bundle();
    for uri in [
        "quench://app/",
        "quench://app",
        "quench://app/users/7",
        "quench://app/users/7?tab=1#top",
    ] {
        let a = b.answer(uri);
        assert_eq!(
            (a.status, a.body.as_slice()),
            (200, &b"<html>app</html>"[..]),
            "{uri}"
        );
        assert_eq!(a.content_type, "text/html; charset=utf-8");
    }
}

#[quench_test::test]
fn files_are_served_with_their_type_and_missing_ones_are_404() {
    let b = bundle();
    let wasm = b.answer("quench://app/_quench/app.1.wasm");
    assert_eq!(
        (wasm.status, wasm.content_type.as_str()),
        (200, "application/wasm")
    );
    assert_eq!(wasm.body, b"\0asm");
    // A leading slash on `with` is dropped.
    assert_eq!(b.answer("quench://app/style.css").body, b"body{}");
    for uri in ["quench://app/missing.js", "quench://app/_quench/other.wasm"] {
        assert_eq!(b.answer(uri).status, 404, "{uri}");
    }
}

#[quench_test::test]
fn other_origins_and_hostile_paths_are_404() {
    let b = bundle();
    for uri in [
        "https://app/",
        "quench://evil/",
        "file:///etc/passwd",
        "quench://app/../secret.txt",
        "quench://app/a/../../x.txt",
        "quench://app/./x.js",
        "quench://app/a\\b.js",
        "quench://app/a%2Fb.js",
        "quench://app/a%5cb.js",
        "nonsense",
        "",
    ] {
        assert_eq!(b.answer(uri).status, 404, "{uri:?}");
    }
}

#[quench_test::test]
fn content_types_follow_the_extension() {
    for (path, want) in [
        ("a.html", "text/html; charset=utf-8"),
        ("a.js", "text/javascript; charset=utf-8"),
        ("a.mjs", "text/javascript; charset=utf-8"),
        ("a.css", "text/css; charset=utf-8"),
        ("a.wasm", "application/wasm"),
        ("a.json", "application/json"),
        ("a.svg", "image/svg+xml"),
        ("a.png", "image/png"),
        ("a.jpg", "image/jpeg"),
        ("a.jpeg", "image/jpeg"),
        ("a.ico", "image/x-icon"),
        ("a.woff2", "font/woff2"),
        ("a.txt", "text/plain; charset=utf-8"),
        ("a.bin", "application/octet-stream"),
    ] {
        assert_eq!(content_type(path), want, "{path}");
    }
}

#[quench_test::test]
fn a_directory_is_read_with_its_subdirectories() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("_quench")).unwrap();
    std::fs::write(dir.path().join("index.html"), "home").unwrap();
    std::fs::write(dir.path().join("_quench/app.js"), "js").unwrap();
    let b = Bundle::from_dir(dir.path()).unwrap();
    assert_eq!((b.len(), b.is_empty()), (2, false));
    assert_eq!(b.get("_quench/app.js"), Some(&b"js"[..]));
    assert_eq!(b.get("nope"), None);
    assert_eq!(b.answer("quench://app/").body, b"home");
    assert!(Bundle::from_dir(&dir.path().join("missing")).is_err());
    assert!(Bundle::new().is_empty());
    assert_eq!(
        Bundle::new().answer("quench://app/").status,
        404,
        "no page, no answer"
    );
}
