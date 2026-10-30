---
title: Re-exports
parent: quench-web
grand_parent: Crates
nav_order: 5
---

The theme, CSS, asset and i18n code moved to `quench-style` (version 0.1.2 at this writing); `quench-web` keeps every old path working. `tests/unit/legacy_paths_tests.rs` is the compile-time proof: it imports each path below. This page lists what is reachable and where; behaviour belongs to `quench-style`, summarised here only as far as `quench-web` depends on it.

## Paths

| Item | Reachable as |
|---|---|
| `CssRule` | `quench_web::styling::css::CssRule`, `quench_web::prelude::CssRule` (there is no `quench_web::css`) |
| `Theme`, `ThemeSpec`, `theme_shared`, `Color`, `Palette`, `GRAY_STEPS`, `SUCCESS_STEPS` | `quench_web::{..}`, `quench_web::framework::{..}`, `quench_web::theme::{..}`, `quench_web::framework::theme::{..}` |
| theme modules `default_dark`, `default_light`, `bootstrap_dark`, `bootstrap_light`, `palette` | `quench_web::{module}`, `quench_web::framework::{module}`, `quench_web::theme::{module}`, `quench_web::framework::theme::{module}` |
| `styles` (and `styles::common::{content, elements, ...}`) | `quench_web::styles`, `quench_web::framework::styles` |
| asset and i18n functions | `quench_web::{fn}`, `quench_web::app::{fn}`, `quench_web::framework::{fn}`, `quench_web::framework::app::{fn}` |

`framework/mod.rs` re-exports `quench_style::{styles, theme}` and glob-exports `theme::*`; `framework/app/mod.rs` glob-exports `quench_style::assets::*` and `quench_style::i18n::*`; `styling/mod.rs` re-exports `quench_style::css`.

## `CssRule`

```rust
#[derive(Debug)]
pub struct CssRule { /* selector, properties, children */ }
impl CssRule {
    #[must_use] pub fn new(selector: &str) -> Self
    #[must_use] pub fn property(self, name: &str, value: &str) -> Self
    #[must_use] pub fn child(self, rule: Self) -> Self
    #[must_use] pub fn render(&self) -> String
}
```

`render` writes `selector {`, one `name: value;` line per property (4 spaces per nesting level), then each child rule **nested inside** (nested CSS, not flattened), then `}`.

```rust
let css = CssRule::new(".a").property("color", "red").child(CssRule::new("&:hover").property("color", "blue")).render();
assert_eq!(css, ".a {\n    color: red;\n    &:hover {\n        color: blue;\n    }\n}\n");
```

## `Theme`

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Theme { DefaultDark, DefaultLight, BootstrapDark, BootstrapLight }
```

`Display` writes `default-dark`, `default-light`, `bootstrap-dark`, `bootstrap-light` (also the CSS file stems and the values stored in the `qtheme` cookie).

| Item | Signature |
|---|---|
| `Theme::ALL` | `pub const ALL: [Self; 4]`, declaration order |
| `Theme::iter` | `pub fn iter() -> impl Iterator<Item = Self>` |
| `Theme::theme` | `#[must_use] pub fn theme(theme: Self) -> String`, the CSS of one theme |
| `Theme::palette` | `#[must_use] pub const fn palette(self) -> Palette` |
| `theme_shared` | `#[must_use] pub fn theme_shared() -> String`, the shared base CSS (`root`, `header`, `content`, `footer`, `elements`, `modal` rules) |
| `trait ThemeSpec` | `fn colors() -> Vec<CssRule>;` and provided `fn render() -> String` |

## Asset functions (`quench_style::assets`)

```rust
pub const FAVICON: &[u8];
pub fn create_asset_files(default_theme: Theme, resources_prefix: &str)
pub fn create_asset_files_with_options(
    _default_theme: Theme,
    supported_themes: &[Theme],
    supported_locales: &[String],
    _resources_prefix: &str,
)
pub fn write_assets(root: &Path, supported_themes: &[Theme], supported_locales: &[String])
```

* `create_asset_files` uses every theme and `available_locales().unwrap_or_default()` and calls `create_asset_files_with_options`.
* `create_asset_files_with_options` **ignores `default_theme` and `resources_prefix`** (both parameters are underscore-prefixed) and calls `write_assets(Path::new("dist/assets"), ..)`.
* `write_assets` writes `css/style.css`, `favicon.png`, `css/themes/{theme}.css` per theme and `js/translations.js` under `root`, creating `css/themes` and `js`. Each failure is printed to stderr (`ERROR: failed to ...`) and the remaining files are still written. No return value.

## i18n functions (`quench_style::i18n`)

```rust
pub fn available_locales() -> Result<Vec<String>>
pub fn available_locales_in(dir: &Path) -> Result<Vec<String>>
pub fn validate_locales_exist(locales: &[String]) -> Result<()>
pub fn validate_locales_exist_in(dir: &Path, locales: &[String]) -> Result<()>
pub fn parse_ftl_with_options(supported_locales: Option<&[String]>) -> Result<String>
pub fn parse_ftl_in(dir: &Path, supported_locales: Option<&[String]>) -> Result<String>
#[must_use] pub fn generate_translations_js(supported_locales: &[String]) -> String
```

(`Result` is `quench_core::Result`.) The `*_in` variants take a directory; the others use `./i18n`, relative to the **process working directory**.

* `available_locales_in`: file stems of `*.ftl` files in `dir`, sorted and de-duplicated; `Ok(vec![])` when `dir` does not exist.
* `validate_locales_exist_in`: `Err("missing locale file: {dir}/{locale}.ftl")` for the first locale without a file.
* `parse_ftl_in`: pretty JSON `{"en-US": {"message-id": "text with {$var}"}}` for the allowed locales (all when `None`), keys sorted; `"{}"` when `dir` does not exist; an `Err` naming the file when an `.ftl` fails to parse.
* `generate_translations_js`: `window.qTranslations = {json};`; a parse error yields `window.qTranslations = ;` (the error is swallowed by `unwrap_or_default()`).
