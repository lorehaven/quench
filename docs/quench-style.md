# Quench Style

`quench-style` (crate `quench_style`) holds everything about how Quench front ends look, so a server-rendered page (`quench-web`), the WASM UI (`quench-ui`) and a mobile or desktop `WebView` share one source of truth. It has no third-party dependencies (only `quench-core` and `quench-serde`) and builds for `wasm32-unknown-unknown`.

Mobile and desktop hosts run the same WASM bundle in a `WebView`, so there is one token set and one theme set. Platform differences are small additive layers on top: `q-m-*` for touch and `q-d-*` for a fine pointer.

## What is in it

| Module | Contents |
|---|---|
| `css` | `CssRule`: a nested-rule builder that renders CSS (`CssRule::new(sel).property(k, v).child(rule).render()`) |
| `theme` | `Theme` (`DefaultDark`, `DefaultLight`, `BootstrapDark`, `BootstrapLight`), `Theme::theme(t)` for a theme's CSS, `theme_shared()` for the shared base CSS, and `Palette`/`Color` for the colour tokens (`Theme::palette()`) |
| `styles` | The shared base rules (`root`, `header`, `content`, `footer`, `elements`, `modal`) and the `q-shell-*` class vocabulary |
| `tokens` | Non-colour design tokens: spacing, radii, type scale, line heights, elevation, durations, easing, touch and pointer target sizes |
| `mobile` | `q-m-*` classes: safe-area insets, 44px touch targets, tap highlight, overscroll and momentum scrolling, dynamic viewport height, motion with reduced-motion support |
| `desktop` | `q-d-*` classes in one `@media (hover: hover) and (pointer: fine)` block: hover, `:focus-visible`, dense controls, thin scrollbars, selectable text |
| `i18n`, `ftl` | Fluent `.ftl` reader and `window.qTranslations` generation (`available_locales`, `generate_translations_js`, `parse_ftl_in`) |
| `components` | `q-*` styles for the `quench-ui` kit (virtual list, app bar, bottom nav, sheet, dialog, snackbar, tiles, switch, checkbox, text field, FAB, chip, tabs, pull to refresh, swipe action, presence) written in the theme's own custom properties, plus `touch-action` classes and `prefers-reduced-motion` handling. `TEXT_PAIRS` and `EDGE_PAIRS` list the colour pairs used, and tests check every one against WCAG contrast (4.5:1 for text, 3:1 for edges and focus rings) in all four themes. `Color::contrast` and `Color::luminance` are public |
| `assets` | `write_assets(root, themes, locales)` and the `dist/assets` writers, plus the `FAVICON` bytes |

## Tokens

Every token is data first. Colours live in a `Palette` per theme (19 `--bs-*` custom properties, the same set for every theme); the rest are `Token<T>` constants (`SPACING`, `RADII`, `TEXT_SIZES`, `LINE_HEIGHTS`, `ELEVATION`, `DURATIONS`, `EASING`, `TOUCH_TARGET`, `POINTER_TARGET`) that `tokens::stylesheet()` publishes as `--q-*` custom properties. The same constants are usable from Rust, for a renderer that does not use CSS.

```rust
use quench_style::{desktop, mobile, theme::Theme, tokens};

let css = [
    tokens::stylesheet(),   // :root { --q-space-1: 0.25rem; ... } + reduced-motion override
    mobile::stylesheet(),   // .q-m-safe, .q-m-touch, ...
    desktop::stylesheet(),  // @media (hover: hover) and (pointer: fine) { .q-d-hover:hover ... }
]
.join("\n");

assert_eq!(Theme::DefaultDark.palette().tokens()[0].0, "--bs-warning");
```

The token, mobile and desktop stylesheets are opt-in: `theme_shared()` and the theme files are byte-identical to what `quench-web` produced before the split (checked by `quench-web`'s golden tests), so existing pages do not change.

Under `prefers-reduced-motion: reduce` the duration tokens collapse to `1ms`, so anything that transitions on `var(--q-dur-*)` stops animating without extra rules.

## `quench-web`

`quench-web` depends on this crate and re-exports it at the old paths (`quench_web::framework::{theme, styles}`, `quench_web::styling::css`, `quench_web::framework::app::{assets, i18n}` items), so services keep compiling unchanged.

## Notes

- `i18n` and `assets` use `std::fs`, which fails at run time on `wasm32-unknown-unknown`. Call them from the server or build tool.
- The fixed pieces of the base stylesheet use `max-width: 768px` queries; `tokens::NARROW_VIEWPORT_PX` documents that value.

## Testing

Unit tests under `tests/unit` plus inline FTL tests: palettes (every theme defines the same tokens, the dark/light ramps run in opposite directions, malformed hex is rejected), tokens (unique `--q-` names, increasing spacing, 44px touch minimum, reduced-motion override), the mobile and desktop layers (namespacing; every desktop rule sits inside the pointer media query; no collision with the shared classes), the asset writer (into a temp dir, including an unwritable root), and the moved theme, style, CSS and i18n suites. Line coverage 98%.

[Home](../README.md)
