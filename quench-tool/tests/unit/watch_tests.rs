use super::support::*;
use quench_tool::watch::{Event, Poller, Watch, fingerprint};
use std::time::Duration;

fn put(dir: &std::path::Path, rel: &str, body: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[quench_test::test]
fn the_fingerprint_follows_sources() {
    let dir = tmp("fingerprint");
    put(&dir, "src/lib.rs", "a");
    let first = fingerprint(&dir);
    assert_eq!(first, fingerprint(&dir));
    put(&dir, "src/lib.rs", "longer");
    let edited = fingerprint(&dir);
    assert_ne!(first, edited);
    put(&dir, "src/new.rs", "");
    let added = fingerprint(&dir);
    assert_ne!(edited, added);
    std::fs::remove_file(dir.join("src/new.rs")).unwrap();
    assert_eq!(fingerprint(&dir), edited);
    put(&dir, "i18n/en.ftl", "hi = Hi");
    put(&dir, "Cargo.toml", "");
    assert_ne!(fingerprint(&dir), edited);
}

#[quench_test::test]
fn build_output_hidden_directories_and_other_files_are_ignored() {
    let dir = tmp("ignored");
    put(&dir, "src/lib.rs", "a");
    let before = fingerprint(&dir);
    for rel in [
        "target/debug/x.rs",
        "dist/app.js",
        ".git/config.toml",
        "node_modules/m.js",
        "notes.txt",
        "image.png",
    ] {
        put(&dir, rel, "ignored");
    }
    assert_eq!(fingerprint(&dir), before);
    assert_eq!(
        fingerprint(&dir.join("does-not-exist")),
        fingerprint(&dir.join("also-missing"))
    );
}

#[quench_test::test]
fn the_poller_is_quiet_until_something_changes_and_then_says_so_once() {
    let dir = tmp("poller");
    put(&dir, "src/lib.rs", "a");
    let mut poller = Poller::new(&dir, Duration::from_millis(10));
    assert_eq!(poller.poll(), Event::Quiet);
    put(&dir, "src/lib.rs", "a longer text");
    assert_eq!(poller.poll(), Event::Changed);
    assert_eq!(poller.poll(), Event::Quiet);
}
