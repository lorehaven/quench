//! Pointer-first additions, namespaced `q-d-*`, for desktop browsers and desktop `WebView` hosts.
//!
//! Every rule sits inside one `@media (hover: hover) and (pointer: fine)` block, so a touch device
//! never sees them. Additive: nothing here changes an existing class.

use crate::css::CssRule;

/// The media query that gates the whole layer.
pub const MEDIA: &str = "@media (hover: hover) and (pointer: fine)";

/// The desktop layer's rules: a single media block.
#[must_use]
pub fn rules() -> Vec<CssRule> {
    vec![
        CssRule::new(MEDIA)
            .child(
                CssRule::new(".q-d-hover:hover")
                    .property("background-color", "var(--bs-gray-800)")
                    .property("cursor", "pointer"),
            )
            .child(
                CssRule::new(".q-d-focus:focus-visible")
                    .property("outline", "2px solid var(--bs-success-500)")
                    .property("outline-offset", "2px"),
            )
            // Tighter controls than the touch-sized defaults.
            .child(
                CssRule::new(".q-d-dense")
                    .property("padding", "var(--q-space-2) var(--q-space-3)")
                    .property("font-size", "var(--q-text-sm)")
                    .property("min-height", "var(--q-pointer-target)"),
            )
            .child(
                CssRule::new(".q-d-scroll")
                    .property("scrollbar-width", "thin")
                    .property("scrollbar-color", "var(--bs-gray-600) transparent"),
            )
            .child(CssRule::new(".q-d-selectable").property("user-select", "text")),
    ]
}

/// The desktop layer as a stylesheet.
#[must_use]
pub fn stylesheet() -> String {
    rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n")
}
