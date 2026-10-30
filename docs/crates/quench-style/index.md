---
title: quench-style
parent: Crates
nav_order: 45
has_children: true
permalink: /crates/quench-style/
---

`quench-style` (crate `quench_style`) holds everything about how Quench front ends look: a tiny CSS rule builder (`CssRule`), four colour themes, design tokens (spacing, radii, type, elevation, motion, target sizes), the shared base stylesheet of the application shell, mobile and desktop layers, a component stylesheet for the `quench-ui` kit, and a Fluent (`.ftl`) translation bundle generator. A server-rendered page ([quench-web](../quench-web/index.md)), the WASM UI ([quench-ui](../quench-ui/index.md)) and a WebView on mobile or desktop all read the same output.

* Version: `0.1.2`. Edition 2024. No features. `unsafe_code = "deny"`; none present.
* The "style engine" is data plus a string renderer: no parsing, no cascade (that is [quench-css](../quench-css/index.md)). `CssRule::render` emits **CSS Nesting** (child rules are written inside their parent, `&` for the parent selector), which modern browsers and `quench-css` read.
* Builds for `wasm32-unknown-unknown`. The functions in `i18n` and `assets` use `std::fs` and fail at run time on that target: call them from the server or build tool only.

## Install

```toml
[dependencies]
quench-style = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Dependencies

| Crate | Why |
|---|---|
| `quench-core` | `Context`, `Result` and `bail!` for `i18n` errors |
| `quench-serde` | `json::to_string_pretty` for `window.qTranslations` |
| `quench-test` (dev) | test harness |

## Module map

| Module | Contents |
|---|---|
| [`css`](api.md#css) | `CssRule` builder and renderer |
| [`theme`](api.md#theme) | `Theme`, `ThemeSpec`, `Palette`, `Color`, `theme_shared()`, the four theme types |
| [`styles`](api.md#styles) | `styles::common::{root, header, content, footer, elements, modal}`: the shared shell rules |
| [`tokens`](api.md#tokens) | non-colour design tokens and their `--q-*` stylesheet |
| [`mobile`](api.md#mobile), [`desktop`](api.md#desktop) | `q-m-*` and `q-d-*` additive layers |
| [`components`](api.md#components) | `q-*` styles of the `quench-ui` kit; contrast pairs |
| [`ftl`](api.md#ftl), [`i18n`](api.md#i18n) | Fluent subset parser, locale discovery, `window.qTranslations` |
| [`assets`](api.md#assets) | writes `dist/assets/...` |

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Configuration](configuration.md): themes, tokens, files and directories it reads and writes.
* [Internals](internals.md): output shape, class vocabulary, palettes, Fluent subset.
* [Errors](errors.md)
* [Testing](testing.md)

## Minimal example

```rust
use quench_style::{css::CssRule, theme::Theme};

let rule = CssRule::new(".card")
    .property("padding", "var(--q-space-3)")
    .child(CssRule::new("&:hover").property("opacity", "0.9"));
assert_eq!(
    rule.render(),
    ".card {\n    padding: var(--q-space-3);\n    &:hover {\n        opacity: 0.9;\n    }\n}\n"
);

let css = Theme::theme(Theme::DefaultDark);        // ":root { --bs-warning: #f59e0b; ... }"
assert!(css.starts_with(":root {\n    --bs-warning: #f59e0b;"));
```
