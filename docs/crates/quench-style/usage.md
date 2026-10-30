---
title: Usage
parent: quench-style
nav_order: 1
---

## 1. Build a stylesheet in Rust

```rust
use quench_style::css::CssRule;

let button = CssRule::new(".btn")
    .property("color", "var(--bs-gray-100)")
    .property("padding", "var(--q-space-2) var(--q-space-4)")
    .child(CssRule::new("&:hover").property("background-color", "var(--bs-success-700)"))
    .child(
        CssRule::new("@media screen and (max-width: 768px)")
            .child(CssRule::new("&").property("width", "100%")),
    );
let css = button.render();
```

`render` indents with four spaces and writes children inside the parent. Nothing is escaped or validated: selectors, names and values are written exactly as given.

## 2. Assemble the CSS a page needs

```rust
use quench_style::{desktop, mobile, theme::{theme_shared, Theme}, tokens, components};

let sheet = [
    Theme::theme(Theme::DefaultDark),   // :root colour tokens + .color-green/yellow/red
    theme_shared(),                     // shell, forms, tables, modals
    tokens::stylesheet(),               // --q-* tokens + reduced-motion override
    components::stylesheet(),           // q-* component styles
    mobile::stylesheet(),               // .q-m-*
    desktop::stylesheet(),              // @media (hover: hover) and (pointer: fine) { .q-d-* }
]
.join("\n");
```

`tokens`, `mobile`, `desktop` and `components` are opt-in; `theme_shared()` and the theme files are what `quench-web` served before the split and did not change.

## 3. Write the asset files for a server

```rust
use quench_style::{assets, theme::Theme};
use std::path::Path;

let locales = vec!["en-US".to_string()];
assets::write_assets(Path::new("dist/assets"), &Theme::ALL, &locales);
// dist/assets/css/style.css            = theme_shared()
// dist/assets/css/themes/<theme>.css   = Theme::theme(theme), one per theme (default-dark.css, ...)
// dist/assets/js/translations.js       = window.qTranslations = {...};
// dist/assets/favicon.png              = assets::FAVICON
```

Failures (cannot create a directory or write a file) are printed to stderr as `ERROR: ...` and the remaining files are still attempted; the function returns `()`.

## 4. Translations from `.ftl` files

Given `i18n/en-US.ftl`:

```text
# greeting
hello = Hello, { $name }!
```

```rust
use quench_style::i18n;

let locales = i18n::available_locales()?;                 // ["en-US"], sorted; [] if ./i18n is missing
i18n::validate_locales_exist(&locales)?;
let js = i18n::generate_translations_js(&locales);
// window.qTranslations = {
//   "en-US": {
//     "hello": "Hello, {$name}!"
//   }
// };
# Ok::<(), quench_core::Error>(())
```

The browser substitutes `{$name}` itself. A parse error in a `.ftl` file makes `generate_translations_js` return `window.qTranslations = ;` (see [Errors](errors.md)): use `parse_ftl_in` when you need the error.

## 5. Check contrast of your own pairs

```rust
use quench_style::theme::{Color, Theme};

let p = Theme::DefaultDark.palette();
let text = p.color("--bs-gray-100").unwrap();
let surface = p.color("--bs-gray-900").unwrap();
assert!(text.contrast(surface) >= 4.5);                 // WCAG body text
let mixed = p.color("mix:--bs-danger:--bs-gray-100:60").unwrap(); // 60 % danger, 40 % gray-100
assert!(Color::mix(Color::hex("#000000"), Color::hex("#ffffff"), 0.5).to_string() == "#808080");
```

## 6. Use tokens from Rust (no CSS)

```rust
use quench_style::tokens::{SPACING, TOUCH_TARGET};

assert_eq!(SPACING[3].name, "--q-space-4");
assert_eq!(SPACING[3].value.px(), 16.0);        // 1rem = 16px
assert_eq!(TOUCH_TARGET.css(), "44px");
```

Pitfall: `body`/`html` in `theme_shared()` set `user-select: none` and `* { color: var(--q-shell-text) }`; text that should be selectable needs `.q-d-selectable` or an explicit rule.
