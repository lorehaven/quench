//! Golden-file tests: the generated HTML, CSS and JS must stay byte-identical across refactors.
//!
//! The baseline lives in `tests/fixtures/baseline/`. After an intended change, regenerate it with
//! `QUENCH_UPDATE_GOLDEN=1 cargo test -p quench-web golden` and review the diff.

use quench_web::framework::app::AppBuilder;
use quench_web::framework::{
    FooterBuilder, HeaderBuilder, NavPanelBuilder, Theme, parse_ftl_in, theme_shared,
};
use quench_web::prelude::*;
use std::path::{Path, PathBuf};

fn baseline_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/baseline")
}

fn locales() -> Vec<String> {
    vec!["en-US".to_string(), "pl-PL".to_string()]
}

/// Compares `actual` with the stored fixture, or rewrites it when `QUENCH_UPDATE_GOLDEN` is set.
fn assert_golden(name: &str, actual: &[u8]) {
    let path = baseline_dir().join(name);
    if std::env::var_os("QUENCH_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read(&path)
        .unwrap_or_else(|err| panic!("missing golden file {}: {err}", path.display()));
    assert!(
        expected == actual,
        "{name} differs from its golden file (regenerate with QUENCH_UPDATE_GOLDEN=1 if intended)"
    );
}

/// The shell `AppShellBuilder` assembles by default, minus the asset-writing side effect.
fn shell_page(content: Element) -> String {
    let themes: Vec<Theme> = Theme::iter().collect();
    let nav = NavPanelBuilder::new()
        .default_theme(Theme::DefaultDark)
        .default_locale("en-US")
        .supported_themes(themes.clone())
        .supported_locales(locales())
        .build();
    AppBuilder::new()
        .title("Quench")
        .supported_themes(themes)
        .supported_locales(locales())
        .default_theme(Theme::DefaultDark)
        .default_locale("en-US")
        .header(
            HeaderBuilder::new()
                .label("Quench - sample web app")
                .with_nav(nav)
                .build(),
        )
        .footer(FooterBuilder::new().label("footer").build())
        .page_content(content)
        .build()
}

#[quench_test::test]
fn shared_stylesheet_is_unchanged() {
    assert_golden("assets/css/style.css", theme_shared().as_bytes());
}

#[quench_test::test]
fn every_theme_stylesheet_is_unchanged() {
    for theme in Theme::iter() {
        assert_golden(
            &format!("assets/css/themes/{theme}.css"),
            Theme::theme(theme).as_bytes(),
        );
    }
}

#[quench_test::test]
fn translations_script_is_unchanged() {
    let json = parse_ftl_in(&baseline_dir().join("i18n"), Some(&locales())).unwrap();
    assert_golden(
        "assets/js/translations.js",
        format!("window.qTranslations = {json};").as_bytes(),
    );
}

#[quench_test::test]
fn favicon_is_unchanged() {
    assert_golden("assets/favicon.png", quench_web::framework::app::FAVICON);
}

/// Mirrors `examples/basic`: a single heading inside the default shell.
#[quench_test::test]
fn basic_example_page_is_unchanged() {
    let page = shell_page(
        content()
            .class("main-content")
            .child(h1().attr("id", "root").text("Hello.")),
    );
    assert_golden("pages/basic.html", page.as_bytes());
}

/// A service-style page: headings, links, a form with every input kind, a list, and a modal
/// toggle, with text that needs escaping.
#[quench_test::test]
fn service_page_is_unchanged() {
    let page = shell_page(
        div()
            .class("page")
            .child(h2().text("Pipelines & <runs>"))
            .child(p().text("5 \"active\" of 'total' 12"))
            .child(a().attr("href", "/ui/runs?x=1&y=2").text("All runs"))
            .child(
                ul().child(li().text("alpha").class("ok"))
                    .child(li().text("beta").class("warn")),
            )
            .child(
                form()
                    .attr("method", "post")
                    .attr("action", "/ui/launch")
                    .child(label().attr("for", "name").text("Name"))
                    .child(input().attr("id", "name").attr("type", "text"))
                    .child(textarea().attr("id", "notes").text("a < b"))
                    .child(
                        select()
                            .attr("id", "env")
                            .child(option().attr("value", "dev").text("dev"))
                            .child(option().attr("value", "prod").text("prod")),
                    )
                    .child(
                        button()
                            .class("btn")
                            .on_click("toggleModal()")
                            .child(i().class("fa-solid fa-play"))
                            .text("Launch"),
                    ),
            )
            .child(pre().text("line 1\n  line 2")),
    );
    assert_golden("pages/service.html", page.as_bytes());
}

#[quench_test::test]
fn rendering_is_deterministic() {
    // Attribute order used to depend on a randomly seeded hash map.
    let first = shell_page(div().text("same"));
    for _ in 0..200 {
        assert_eq!(first, shell_page(div().text("same")));
    }
}
