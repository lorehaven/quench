# quench-style
Shared styling: design tokens, themes, base CSS and i18n catalogs for web, mobile and desktop.

## What it is
`quench-style` holds everything about how Quench front ends look: a tiny CSS rule builder (`CssRule`), four colour themes, design tokens (spacing, radii, type, elevation, motion, target sizes), the shared base stylesheet of the application shell, mobile and desktop layers, a component stylesheet for the `quench-ui` kit, and a Fluent (`.ftl`) translation bundle generator.

A server-rendered page (`quench-web`), the WASM UI (`quench-ui`) and a WebView on mobile or desktop all read the same output. It is data plus a string renderer, with no parsing or cascade (that is `quench-css`), and it builds for `wasm32-unknown-unknown`.

## Use cases
- Build a stylesheet in Rust with `CssRule`, using CSS nesting.
- Assemble the theme, token, component, mobile and desktop CSS a page needs.
- Write the `dist/assets` files for a server with `assets::write_assets`.
- Generate `window.qTranslations` from `.ftl` files.
- Check contrast of colour pairs in a theme palette.

## Example
```rust
use quench_style::css::CssRule;

let button = CssRule::new(".btn")
    .property("color", "var(--bs-gray-100)")
    .property("padding", "var(--q-space-2) var(--q-space-4)")
    .child(CssRule::new("&:hover").property("background-color", "var(--bs-success-700)"));
let css = button.render();
```

```rust
use quench_style::{components, theme::{theme_shared, Theme}, tokens};

let sheet = [
    Theme::theme(Theme::DefaultDark),
    theme_shared(),
    tokens::stylesheet(),
    components::stylesheet(),
]
.join("\n");
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-style/
