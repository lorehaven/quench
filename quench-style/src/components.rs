//! Styles for the components of `quench-ui`'s kit.
//!
//! Classes are namespaced `q-*` after the component (`q-vlist`, `q-appbar`, ...). They work on every
//! platform; the touch and desktop layers ([`crate::mobile`], [`crate::desktop`]) refine them.
//!
//! The colours are the theme's custom properties, so the same stylesheet serves every theme.

use crate::css::CssRule;

/// The text and surface colours the components use, as `(text, surface)` palette tokens. Every
/// pair meets the WCAG contrast for body text (4.5:1) in every theme; a test checks it.
pub const TEXT_PAIRS: &[(&str, &str)] = &[
    ("--bs-gray-100", "--bs-gray-950"),
    ("--bs-gray-100", "--bs-gray-900"),
    ("--bs-gray-100", "--bs-gray-800"),
    ("--bs-gray-300", "--bs-gray-950"),
    ("--bs-gray-300", "--bs-gray-900"),
    ("white", "--bs-success-800"),
    (DANGER_TEXT_TOKEN, "--bs-gray-950"),
    (DANGER_TEXT_TOKEN, "--bs-gray-900"),
];

/// Error text, spelled for [`crate::theme::Palette::color`].
///
/// The danger colour pulled towards the theme's text colour, which is the direction that raises
/// contrast in both light and dark themes (the plain danger colour reads below 4.5:1 on the dark
/// backgrounds).
pub const DANGER_TEXT_TOKEN: &str = "mix:--bs-danger:--bs-gray-100:60";
const DANGER_TEXT: &str = "color-mix(in srgb, var(--bs-danger) 60%, var(--bs-gray-100))";

/// The edges of controls and focus rings against what is behind them, which need 3:1.
pub const EDGE_PAIRS: &[(&str, &str)] = &[
    ("--bs-gray-400", "--bs-gray-950"),
    ("--bs-gray-400", "--bs-gray-900"),
    ("--bs-gray-100", "--bs-gray-950"),
    ("--bs-gray-100", "--bs-gray-900"),
];

/// The rules of every component.
#[must_use]
pub fn rules() -> Vec<CssRule> {
    let mut all = Vec::new();
    all.extend(virtual_list());
    all.extend(animation());
    all.extend(touch());
    all.extend(bars());
    all.extend(modals());
    all.extend(controls());
    all.extend(fields());
    all.extend(lists());
    all.extend(gestures());
    all
}

/// The rules as a stylesheet.
#[must_use]
pub fn stylesheet() -> String {
    rules()
        .iter()
        .map(CssRule::render)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A long list that builds only the rows in view. Needs a height from the page.
fn virtual_list() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-vlist")
            .property("position", "relative")
            .property("overflow-y", "auto")
            .property("overflow-anchor", "none")
            .property("overscroll-behavior", "contain"),
        CssRule::new(".q-vlist-space").property("position", "relative"),
        CssRule::new(".q-vlist-window")
            .property("position", "absolute")
            .property("top", "0")
            .property("left", "0")
            .property("right", "0")
            .property("will-change", "transform"),
        CssRule::new(".q-vlist-row")
            .property("box-sizing", "border-box")
            .property("height", "var(--q-row-h)")
            .property("overflow", "hidden"),
    ]
}

const BG: &str = "var(--bs-gray-950)";
const SURFACE: &str = "var(--bs-gray-900)";
const TEXT: &str = "var(--bs-gray-100)";
const MUTED: &str = "var(--bs-gray-300)";
const EDGE: &str = "var(--bs-gray-400)";
const PRIMARY: &str = "var(--bs-success-800)";
/// Focus rings and the mark of the selected item: the text colour, which every theme keeps
/// far from its background.
const RING: &str = TEXT;
const ON_PRIMARY: &str = "white";
const DANGER: &str = "var(--bs-danger)"; // fills and borders, not text

/// A control the size of a fingertip.
fn tappable(selector: &str) -> CssRule {
    CssRule::new(selector)
        .property("min-height", "var(--q-touch-target, 48px)")
        .property("min-width", "var(--q-touch-target, 48px)")
        .property("font", "inherit")
        .property("color", TEXT)
        .property("cursor", "pointer")
        .property("-webkit-tap-highlight-color", "transparent")
        .child(
            CssRule::new("&:focus-visible")
                .property("outline", &format!("3px solid {RING}"))
                .property("outline-offset", "2px"),
        )
}

/// Presence: the two states a CSS transition runs between.
fn animation() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-anim")
            .property("opacity", "0")
            .property("transition", "opacity var(--q-dur-base, 200ms) var(--q-ease-standard, ease), transform var(--q-dur-base, 200ms) var(--q-ease-standard, ease)"),
        CssRule::new(".q-anim.q-anim-in").property("opacity", "1"),
        CssRule::new("@media (prefers-reduced-motion: reduce)").child(
            CssRule::new(".q-anim, .q-sheet, .q-dialog, .q-snackbar, .q-switch-thumb, .q-ptr-indicator, .q-swipe-front")
                .property("transition", "none"),
        ),
    ]
}

/// `touch-action` for gestures, which an inline style could not set under a strict policy.
fn touch() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-touch-none").property("touch-action", "none"),
        CssRule::new(".q-touch-pan-y").property("touch-action", "pan-y"),
        CssRule::new(".q-touch-pan-x").property("touch-action", "pan-x"),
    ]
}

fn bars() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-appbar")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "var(--q-space-2, 0.5rem)")
            .property("min-height", "56px")
            .property("padding", "0 var(--q-space-2, 0.5rem)")
            .property("padding-top", "env(safe-area-inset-top)")
            .property("background", SURFACE)
            .property("color", TEXT)
            .property("border-bottom", &format!("1px solid {EDGE}"))
            .child(
                CssRule::new(".q-appbar-title")
                    .property("flex", "1")
                    .property("margin", "0")
                    .property("font-size", "var(--q-text-lg, 1.125rem)"),
            )
            .child(CssRule::new(".q-appbar-actions").property("display", "flex")),
        tappable(".q-iconbtn")
            .property("background", "transparent")
            .property("border", "0")
            .property("border-radius", "var(--q-radius-full, 999px)")
            .property("font-size", "1.25rem"),
        CssRule::new(".q-bottomnav")
            .property("display", "flex")
            .property("background", SURFACE)
            .property("border-top", &format!("1px solid {EDGE}"))
            .property("padding-bottom", "env(safe-area-inset-bottom)"),
        tappable(".q-bottomnav-item")
            .property("flex", "1")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("background", "transparent")
            .property("border", "0")
            .property("font-size", "var(--q-text-xs, 0.75rem)")
            .child(
                CssRule::new("&[aria-current=\"page\"]")
                    .property("font-weight", "700")
                    .property("box-shadow", &format!("inset 0 -3px 0 {RING}")),
            ),
        tappable(".q-fab")
            .property("position", "fixed")
            .property("right", "calc(16px + env(safe-area-inset-right))")
            // Above a bottom navigation bar, when the page sets `--q-fab-offset` to its height.
            .property(
                "bottom",
                "calc(var(--q-fab-offset, 16px) + env(safe-area-inset-bottom))",
            )
            .property("width", "56px")
            .property("height", "56px")
            .property("background", PRIMARY)
            .property("color", ON_PRIMARY)
            .property("border", "0")
            .property(
                "box-shadow",
                "var(--q-elevation-3, 0 4px 12px rgb(0 0 0 / 0.35))",
            ),
    ]
}

fn modals() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-modal")
            .property("position", "fixed")
            .property("inset", "0")
            .property("z-index", "100"),
        CssRule::new(".q-scrim")
            .property("position", "absolute")
            .property("inset", "0")
            .property("background", "rgb(0 0 0 / 0.5)"),
        CssRule::new(".q-sheet")
            .property("position", "absolute")
            .property("left", "0")
            .property("right", "0")
            .property("bottom", "0")
            .property("max-height", "85dvh")
            .property("overflow-y", "auto")
            .property("padding", "var(--q-space-4, 1rem)")
            .property(
                "padding-bottom",
                "calc(var(--q-space-4, 1rem) + env(safe-area-inset-bottom))",
            )
            .property("background", SURFACE)
            .property("color", TEXT)
            .property(
                "border-radius",
                "var(--q-radius-lg, 1rem) var(--q-radius-lg, 1rem) 0 0",
            )
            .property("transform", "translateY(100%)")
            .property(
                "transition",
                "transform var(--q-dur-base, 200ms) var(--q-ease-standard, ease)",
            ),
        CssRule::new(".q-anim-in .q-sheet").property("transform", "none"),
        CssRule::new(".q-dialog")
            .property("position", "absolute")
            .property("top", "50%")
            .property("left", "50%")
            .property("width", "min(90vw, 28rem)")
            .property("padding", "var(--q-space-5, 1.25rem)")
            .property("background", SURFACE)
            .property("color", TEXT)
            .property("border-radius", "var(--q-radius-lg, 1rem)")
            .property("transform", "translate(-50%, -48%) scale(0.96)")
            .property(
                "transition",
                "transform var(--q-dur-base, 200ms) var(--q-ease-standard, ease)",
            ),
        CssRule::new(".q-anim-in .q-dialog").property("transform", "translate(-50%, -50%)"),
        CssRule::new(".q-modal-title")
            .property("margin", "0 0 var(--q-space-3, 0.75rem)")
            .property("font-size", "var(--q-text-lg, 1.125rem)"),
        CssRule::new(".q-dialog-actions")
            .property("display", "flex")
            .property("justify-content", "flex-end")
            .property("gap", "var(--q-space-2, 0.5rem)")
            .property("margin-top", "var(--q-space-4, 1rem)"),
        CssRule::new(".q-snackbar")
            .property("position", "fixed")
            .property("left", "50%")
            .property("transform", "translateX(-50%)")
            .property("bottom", "calc(72px + env(safe-area-inset-bottom))")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "var(--q-space-3, 0.75rem)")
            .property("max-width", "min(92vw, 32rem)")
            .property(
                "padding",
                "var(--q-space-3, 0.75rem) var(--q-space-4, 1rem)",
            )
            .property("background", "var(--bs-gray-800)")
            .property("color", TEXT)
            .property("border-radius", "var(--q-radius-md, 0.5rem)")
            .property("z-index", "110"),
        tappable(".q-snackbar-action")
            .property("background", "transparent")
            .property("border", "0")
            .property("font-weight", "700")
            .property("text-transform", "uppercase"),
    ]
}

fn controls() -> Vec<CssRule> {
    vec![
        tappable(".q-switch")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("gap", "var(--q-space-3, 0.75rem)")
            .property("background", "transparent")
            .property("border", "0")
            .child(
                CssRule::new(".q-switch-track")
                    .property("position", "relative")
                    .property("width", "52px")
                    .property("height", "32px")
                    .property("border-radius", "16px")
                    .property("background", "var(--bs-gray-700)")
                    .property("border", &format!("2px solid {EDGE}"))
                    .property("box-sizing", "border-box"),
            )
            .child(
                CssRule::new(".q-switch-thumb")
                    .property("position", "absolute")
                    .property("top", "3px")
                    .property("left", "3px")
                    .property("width", "22px")
                    .property("height", "22px")
                    .property("border-radius", "50%")
                    .property("background", TEXT)
                    .property("transition", "transform var(--q-dur-fast, 120ms) ease"),
            )
            .child(
                CssRule::new("&[aria-checked=\"true\"] .q-switch-track")
                    .property("background", PRIMARY)
                    .property("border-color", PRIMARY),
            )
            .child(
                CssRule::new("&[aria-checked=\"true\"] .q-switch-thumb")
                    .property("transform", "translateX(20px)")
                    .property("background", ON_PRIMARY),
            ),
        CssRule::new(".q-checkbox")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("gap", "var(--q-space-3, 0.75rem)")
            .property("min-height", "var(--q-touch-target, 48px)")
            .property("cursor", "pointer")
            .child(
                CssRule::new("input")
                    .property("width", "24px")
                    .property("height", "24px")
                    .property("accent-color", PRIMARY),
            ),
    ]
}

fn fields() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-textfield")
            .property("display", "flex")
            .property("flex-direction", "column")
            .property("gap", "var(--q-space-1, 0.25rem)")
            .property("margin", "var(--q-space-3, 0.75rem) 0")
            .child(
                CssRule::new(".q-textfield-label")
                    .property("font-size", "var(--q-text-sm, 0.875rem)")
                    .property("color", MUTED),
            )
            .child(
                CssRule::new(".q-textfield-input")
                    .property("min-height", "var(--q-touch-target, 48px)")
                    .property("padding", "0 var(--q-space-3, 0.75rem)")
                    .property("font", "inherit")
                    .property("color", TEXT)
                    .property("background", SURFACE)
                    .property("border", &format!("2px solid {EDGE}"))
                    .property("border-radius", "var(--q-radius-md, 0.5rem)")
                    .child(
                        CssRule::new("&:focus-visible")
                            .property("outline", &format!("3px solid {RING}"))
                            .property("outline-offset", "1px"),
                    )
                    .child(
                        CssRule::new("&[aria-invalid=\"true\"]").property("border-color", DANGER),
                    ),
            )
            .child(
                CssRule::new(".q-textfield-hint")
                    .property("margin", "0")
                    .property("font-size", "var(--q-text-sm, 0.875rem)")
                    .property("color", MUTED),
            ),
        CssRule::new(".q-field-error, .q-form-error, .q-textfield-error")
            .property("margin", "0")
            .property("color", DANGER_TEXT)
            .property("font-size", "var(--q-text-sm, 0.875rem)"),
        tappable(".q-chip")
            .property("display", "inline-flex")
            .property("align-items", "center")
            .property("justify-content", "center")
            .property("padding", "0 var(--q-space-4, 1rem)")
            .property("background", "transparent")
            .property("border", &format!("2px solid {EDGE}"))
            .property("border-radius", "var(--q-radius-full, 999px)")
            .child(
                CssRule::new("&.q-chip-on")
                    .property("background", PRIMARY)
                    .property("border-color", PRIMARY)
                    .property("color", ON_PRIMARY),
            ),
        CssRule::new(".q-tablist")
            .property("display", "flex")
            .property("overflow-x", "auto")
            .property("scrollbar-width", "thin")
            .property("border-bottom", &format!("1px solid {EDGE}")),
        tappable(".q-tab")
            .property("flex", "1 0 auto")
            .property("padding", "0 var(--q-space-3, 0.75rem)")
            .property("background", "transparent")
            .property("border", "0")
            .property("border-bottom", "3px solid transparent")
            .child(
                CssRule::new("&[aria-selected=\"true\"]")
                    .property("border-bottom-color", RING)
                    .property("font-weight", "700"),
            ),
        CssRule::new(".q-tabpanel")
            .property("padding", "var(--q-space-3, 0.75rem) 0")
            .property("color", TEXT)
            .child(
                CssRule::new("&:focus-visible").property("outline", &format!("3px solid {RING}")),
            ),
    ]
}

fn lists() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-list")
            .property("list-style", "none")
            .property("margin", "0")
            .property("padding", "0")
            .property("background", BG),
        CssRule::new(".q-list > li").property("border-bottom", "1px solid var(--bs-gray-800)"),
        tappable(".q-tile")
            .property("display", "flex")
            .property("align-items", "center")
            .property("gap", "var(--q-space-3, 0.75rem)")
            .property("width", "100%")
            .property("min-height", "56px")
            .property("padding", "var(--q-space-2, 0.5rem) var(--q-space-4, 1rem)")
            .property("background", "transparent")
            .property("border", "0")
            .property("text-align", "left")
            .property("box-sizing", "border-box"),
        CssRule::new(".q-tile-text")
            .property("flex", "1")
            .property("display", "flex")
            .property("flex-direction", "column"),
        CssRule::new(".q-tile-subtitle")
            .property("color", MUTED)
            .property("font-size", "var(--q-text-sm, 0.875rem)"),
    ]
}

fn gestures() -> Vec<CssRule> {
    vec![
        CssRule::new(".q-ptr")
            .property("position", "relative")
            .property("overflow-y", "auto")
            .property("overscroll-behavior-y", "contain"),
        // At the top the browser keeps the upward direction (scrolling) and the app takes the pull down.
        CssRule::new(".q-ptr.q-ptr-top").property("touch-action", "pan-x pan-up"),
        CssRule::new(".q-ptr:not(.q-ptr-top)").property("touch-action", "pan-y"),
        CssRule::new(".q-ptr-indicator")
            .property("position", "absolute")
            .property("top", "0")
            .property("left", "50%")
            .property("width", "32px")
            .property("height", "32px")
            .property("margin-left", "-16px")
            .property("border-radius", "50%")
            .property("border", &format!("3px solid {PRIMARY}"))
            .property("border-top-color", "transparent")
            .property("transform", "translateY(calc(var(--q-pull, 0px) - 40px))")
            .property("opacity", "0")
            .child(CssRule::new("&[aria-hidden=\"false\"]").property("opacity", "1")),
        CssRule::new(".q-ptr-content").property("transform", "translateY(var(--q-pull, 0px))"),
        CssRule::new(".q-ptr-busy")
            .property("transform", "translateY(16px)")
            .property("animation", "q-spin 0.8s linear infinite"),
        CssRule::new("@keyframes q-spin").child(CssRule::new("to").property("rotate", "360deg")),
        CssRule::new("@media (prefers-reduced-motion: reduce)")
            .child(CssRule::new(".q-ptr-busy").property("animation", "none")),
        CssRule::new(".q-swipe")
            .property("position", "relative")
            .property("overflow", "hidden"),
        CssRule::new(".q-swipe-back")
            .property("position", "absolute")
            .property("inset", "0 0 0 auto")
            .property("display", "flex")
            .property("width", "88px")
            .property("background", DANGER),
        tappable(".q-swipe-action")
            .property("flex", "1")
            .property("background", "transparent")
            .property("border", "0")
            .property("color", "white")
            .property("font-weight", "700"),
        CssRule::new(".q-swipe-front")
            .property("position", "relative")
            .property("background", BG)
            .property("transition", "transform var(--q-dur-fast, 120ms) ease"),
    ]
}
