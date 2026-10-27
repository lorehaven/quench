//! The components' stylesheet: well formed, and readable in every theme.

use quench_style::components::{EDGE_PAIRS, TEXT_PAIRS, rules, stylesheet};
use quench_style::theme::Theme;

#[quench_test::test]
fn every_text_pair_meets_the_body_text_contrast_in_every_theme() {
    for theme in Theme::ALL {
        let palette = theme.palette();
        for (fg, bg) in TEXT_PAIRS {
            let (a, b) = (palette.color(fg).unwrap(), palette.color(bg).unwrap());
            let ratio = a.contrast(b);
            assert!(ratio >= 4.5, "{theme}: {fg} on {bg} is {ratio:.2}:1");
        }
    }
}

#[quench_test::test]
fn control_edges_and_focus_rings_meet_the_graphics_contrast_in_every_theme() {
    for theme in Theme::ALL {
        let palette = theme.palette();
        for (fg, bg) in EDGE_PAIRS {
            let ratio = palette
                .color(fg)
                .unwrap()
                .contrast(palette.color(bg).unwrap());
            assert!(ratio >= 3.0, "{theme}: {fg} against {bg} is {ratio:.2}:1");
        }
    }
}

#[quench_test::test]
fn contrast_is_the_wcag_ratio() {
    let theme = Theme::DefaultLight.palette();
    let white = theme.color("white").unwrap();
    let black = quench_style::theme::Color::rgb(0);
    assert!((white.contrast(black) - 21.0).abs() < 1e-9);
    assert!((white.contrast(white) - 1.0).abs() < 1e-9);
    assert!(theme.color("--bs-nope").is_none());
    assert!((white.luminance() - 1.0).abs() < 1e-9 && black.luminance() == 0.0);
}

#[quench_test::test]
fn the_stylesheet_has_a_rule_for_every_component_and_balanced_braces() {
    let css = stylesheet();
    for class in [
        ".q-vlist",
        ".q-appbar",
        ".q-bottomnav",
        ".q-sheet",
        ".q-dialog",
        ".q-snackbar",
        ".q-tile",
        ".q-switch",
        ".q-checkbox",
        ".q-textfield",
        ".q-fab",
        ".q-chip",
        ".q-tab",
        ".q-ptr",
        ".q-swipe",
        ".q-anim",
        ".q-touch-none",
    ] {
        assert!(css.contains(class), "{class}");
    }
    assert_eq!(css.matches('{').count(), css.matches('}').count());
    assert!(css.contains("prefers-reduced-motion"));
    assert!(
        !css.contains("!important") && !css.contains("url("),
        "no external loads"
    );
    assert!(rules().len() > 40);
}

#[quench_test::test]
fn touch_targets_are_at_least_48_pixels_and_nothing_is_inline() {
    let css = stylesheet();
    assert!(css.contains("min-height: var(--q-touch-target, 48px)"));
    assert!(!css.contains("@import"));
}
