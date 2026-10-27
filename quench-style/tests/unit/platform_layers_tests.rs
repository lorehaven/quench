//! Unit tests for `mobile.rs` and `desktop.rs`.

use quench_style::{desktop, mobile};

#[quench_test::test]
fn mobile_rules_are_namespaced_and_cover_the_touch_concerns() {
    let css = mobile::stylesheet();
    for needle in [
        "env(safe-area-inset-top)",
        "env(safe-area-inset-bottom)",
        "min-height: var(--q-touch-target)",
        "-webkit-tap-highlight-color: transparent",
        "overscroll-behavior: contain",
        "-webkit-overflow-scrolling: touch",
        "100dvh",
        "@media (prefers-reduced-motion: reduce)",
    ] {
        assert!(css.contains(needle), "missing {needle}");
    }
}

#[quench_test::test]
fn every_mobile_selector_is_q_m_or_a_media_or_supports_wrapper() {
    for rule in mobile::rules() {
        let css = rule.render();
        let selector = css.lines().next().unwrap();
        assert!(
            selector.starts_with(".q-m-") || selector.starts_with("@media"),
            "{selector}"
        );
    }
}

#[quench_test::test]
fn every_desktop_rule_sits_behind_the_pointer_media_query() {
    let rules = desktop::rules();
    assert_eq!(rules.len(), 1, "one top-level block");
    let css = rules[0].render();
    assert!(css.starts_with("@media (hover: hover) and (pointer: fine) {"));
    // Everything else is indented one level, i.e. nested inside the block.
    assert!(
        css.lines()
            .skip(1)
            .all(|l| l.is_empty() || l.starts_with("    ") || l == "}")
    );
    let top_level = css
        .lines()
        .filter(|l| !l.starts_with(' ') && !l.is_empty())
        .count();
    assert_eq!(
        top_level, 2,
        "only the opening and closing lines are unindented"
    );
}

#[quench_test::test]
fn desktop_rules_are_namespaced_and_cover_the_pointer_concerns() {
    let css = desktop::stylesheet();
    for needle in [
        ".q-d-hover:hover",
        ".q-d-focus:focus-visible",
        ".q-d-dense",
        "scrollbar-width: thin",
        "user-select: text",
    ] {
        assert!(css.contains(needle), "missing {needle}");
    }
    // Every selector in the block is q-d-*.
    let selectors: Vec<_> = css
        .lines()
        .filter(|l| l.starts_with("    .") && l.ends_with('{'))
        .collect();
    assert!(!selectors.is_empty());
    assert!(
        selectors
            .iter()
            .all(|l| l.trim_start().starts_with(".q-d-"))
    );
}

#[quench_test::test]
fn neither_layer_redefines_a_class_the_shared_stylesheet_owns() {
    let shared = quench_style::theme::theme_shared();
    for css in [mobile::stylesheet(), desktop::stylesheet()] {
        for line in css.lines().filter(|l| l.trim_start().starts_with(".q-")) {
            let class = line.trim().trim_end_matches('{').trim();
            let class = class.split(':').next().unwrap();
            assert!(!shared.contains(&format!("{class} {{")), "{class}");
        }
    }
}
