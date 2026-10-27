//! Touch-first additions, namespaced `q-m-*`. Additive: nothing here changes an existing class.
//!
//! The classes assume the tokens from [`crate::tokens`] and the theme palette are loaded.

use crate::css::CssRule;

/// The mobile layer's rules.
#[must_use]
pub fn rules() -> Vec<CssRule> {
    vec![
        // Keep content clear of notches, rounded corners and system bars.
        CssRule::new(".q-m-safe")
            .property("padding-top", "env(safe-area-inset-top)")
            .property("padding-right", "env(safe-area-inset-right)")
            .property("padding-bottom", "env(safe-area-inset-bottom)")
            .property("padding-left", "env(safe-area-inset-left)"),
        CssRule::new(".q-m-safe-top").property("padding-top", "env(safe-area-inset-top)"),
        CssRule::new(".q-m-safe-bottom").property("padding-bottom", "env(safe-area-inset-bottom)"),
        CssRule::new(".q-m-safe-x")
            .property("padding-left", "env(safe-area-inset-left)")
            .property("padding-right", "env(safe-area-inset-right)"),
        // A comfortable tap area.
        CssRule::new(".q-m-touch")
            .property("min-width", "var(--q-touch-target)")
            .property("min-height", "var(--q-touch-target)")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("justify-content", "center"),
        CssRule::new(".q-m-tap")
            .property("-webkit-tap-highlight-color", "transparent")
            .property("touch-action", "manipulation"),
        CssRule::new(".q-m-select-none")
            .property("-webkit-touch-callout", "none")
            .property("user-select", "none"),
        // A scroll area that keeps momentum and does not drag the page along at its ends.
        CssRule::new(".q-m-scroll")
            .property("overflow-y", "auto")
            .property("-webkit-overflow-scrolling", "touch")
            .property("overscroll-behavior", "contain"),
        CssRule::new(".q-m-no-bounce").property("overscroll-behavior", "none"),
        // The visible viewport, excluding collapsing browser bars.
        CssRule::new(".q-m-viewport")
            .property("height", "100vh")
            .child(
                CssRule::new("@supports (height: 100dvh)")
                    .child(CssRule::new("&").property("height", "100dvh")),
            ),
        // Transitions on the token durations, off under reduced motion.
        CssRule::new(".q-m-motion")
            .property(
                "transition-property",
                "transform, opacity, background-color",
            )
            .property("transition-duration", "var(--q-dur-base)")
            .property("transition-timing-function", "var(--q-ease-standard)"),
        CssRule::new("@media (prefers-reduced-motion: reduce)").child(
            CssRule::new(".q-m-motion")
                .property("transition", "none")
                .property("animation", "none"),
        ),
    ]
}

/// The mobile layer as a stylesheet.
#[must_use]
pub fn stylesheet() -> String {
    rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n")
}
