//! Unit tests for `tokens.rs`.

use quench_style::tokens::{self, Length, Millis, Number};
use std::collections::HashSet;

#[quench_test::test]
fn lengths_numbers_and_durations_format_for_css() {
    assert_eq!(Length::Rem(0.25).to_string(), "0.25rem");
    assert_eq!(Length::Rem(1.0).to_string(), "1rem");
    assert_eq!(Length::Px(44.0).to_string(), "44px");
    assert_eq!(Number(1.5).to_string(), "1.5");
    assert_eq!(Millis(200).to_string(), "200ms");
}

#[quench_test::test]
fn lengths_convert_to_pixels_with_a_16px_rem() {
    assert!((Length::Rem(0.25).px() - 4.0).abs() < f32::EPSILON);
    assert!((Length::Px(44.0).px() - 44.0).abs() < f32::EPSILON);
}

#[quench_test::test]
fn every_token_is_namespaced_and_unique() {
    let all = tokens::all();
    let names: HashSet<_> = all.iter().map(|(name, _)| *name).collect();
    assert_eq!(names.len(), all.len(), "duplicate token name");
    assert!(
        all.iter()
            .all(|(name, value)| name.starts_with("--q-") && !value.is_empty())
    );
}

#[quench_test::test]
fn spacing_is_a_strictly_increasing_scale() {
    let px: Vec<f32> = tokens::SPACING.iter().map(|t| t.value.px()).collect();
    assert!(px.windows(2).all(|w| w[0] < w[1]), "{px:?}");
    assert!(
        (px[0] - 4.0).abs() < f32::EPSILON,
        "scale starts on the 4px grid"
    );
}

#[quench_test::test]
fn the_type_scale_matches_the_shared_headings_and_the_radius_matches_the_panels() {
    assert_eq!(tokens::TEXT_SIZES[6].css(), "3rem");
    assert_eq!(tokens::TEXT_SIZES[5].css(), "2rem");
    assert_eq!(tokens::RADII[1].css(), "0.35rem");
}

#[quench_test::test]
fn touch_targets_meet_the_44px_minimum_and_pointer_targets_are_smaller() {
    assert!(tokens::TOUCH_TARGET.value.px() >= 44.0);
    assert!(tokens::POINTER_TARGET.value.px() < tokens::TOUCH_TARGET.value.px());
    assert_eq!(tokens::NARROW_VIEWPORT_PX, 768);
}

#[quench_test::test]
fn the_stylesheet_publishes_the_tokens_and_a_reduced_motion_override() {
    let css = tokens::stylesheet();
    assert!(css.starts_with(":root {\n    --q-space-1: 0.25rem;"));
    assert!(css.contains("--q-dur-base: 200ms;"));
    assert!(css.contains("--q-touch-target: 44px;"));
    let (_, reduced) = css
        .split_once("@media (prefers-reduced-motion: reduce) {")
        .unwrap();
    for t in tokens::DURATIONS {
        assert!(reduced.contains(&format!("{}: 1ms;", t.name)), "{}", t.name);
    }
}

#[quench_test::test]
fn elevation_and_easing_tokens_are_raw_css_values() {
    assert_eq!(tokens::ELEVATION[0].value, "none");
    assert!(
        tokens::ELEVATION[1..]
            .iter()
            .all(|t| t.value.contains("rgba"))
    );
    assert!(
        tokens::EASING
            .iter()
            .all(|t| t.value.starts_with("cubic-bezier("))
    );
}
