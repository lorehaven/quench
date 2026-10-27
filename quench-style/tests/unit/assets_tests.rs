//! Unit tests for `assets.rs`.

use quench_core::tmp::TempDir;
use quench_style::assets::{FAVICON, write_assets};
use quench_style::theme::{Theme, theme_shared};

#[quench_test::test]
fn write_assets_creates_the_full_tree_under_the_given_root() {
    let dir = TempDir::new().unwrap();
    write_assets(dir.path(), &Theme::ALL, &[]);

    let read = |p: &str| std::fs::read_to_string(dir.path().join(p)).unwrap();
    assert_eq!(read("css/style.css"), theme_shared());
    for theme in Theme::iter() {
        assert_eq!(
            read(&format!("css/themes/{theme}.css")),
            Theme::theme(theme)
        );
    }
    assert_eq!(read("js/translations.js"), "window.qTranslations = {};");
    assert_eq!(
        std::fs::read(dir.path().join("favicon.png")).unwrap(),
        FAVICON
    );
}

#[quench_test::test]
fn only_the_requested_themes_are_written() {
    let dir = TempDir::new().unwrap();
    write_assets(dir.path(), &[Theme::BootstrapLight], &[]);
    assert!(dir.path().join("css/themes/bootstrap-light.css").exists());
    assert!(!dir.path().join("css/themes/default-dark.css").exists());
}

#[quench_test::test]
fn the_favicon_is_a_png() {
    assert_eq!(&FAVICON[..8], b"\x89PNG\r\n\x1a\n");
}

#[quench_test::test]
fn an_unwritable_root_is_reported_without_panicking() {
    let dir = TempDir::new().unwrap();
    // A file where the root directory should be: every write fails, none panics.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, b"x").unwrap();
    write_assets(&blocked, &[Theme::DefaultDark], &[]);
    assert!(blocked.is_file());
}
