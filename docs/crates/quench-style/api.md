---
title: API reference
parent: quench-style
nav_order: 2
---

Modules in `lib.rs` order: `assets`, `components`, `css`, `desktop`, `ftl`, `i18n`, `mobile`, `styles`, `theme`, `tokens`. All types are plain data (`Send + Sync`).

## `css`

```rust
#[derive(Debug)]
pub struct CssRule { /* selector: String, properties: Vec<(String, String)>, children: Vec<CssRule> */ }
impl CssRule {
    #[must_use] pub fn new(selector: &str) -> Self;
    #[must_use] pub fn property(mut self, name: &str, value: &str) -> Self;
    #[must_use] pub fn child(mut self, rule: Self) -> Self;
    #[must_use] pub fn render(&self) -> String;
}
```

* `new`: empty rule. Selectors may be anything: `.a`, `:root`, `@media (...)`, `@keyframes x`, `&:hover`, `&`.
* `property`: appends a declaration (order kept, duplicates kept, nothing validated or escaped).
* `child`: appends a nested rule. Nesting is rendered inside the parent (CSS Nesting); `@media`/`@supports` rules are children like any other.
* `render`: four spaces per level; `selector {\n`, one `name: value;` line per property, then children, then `}\n`. No blank line between sibling rules (callers join rules with `"\n"`, which yields one blank line).

Not `Clone`; has no `Display`.

## `theme`

```rust
pub use palette::{Color, GRAY_STEPS, Palette, SUCCESS_STEPS};
pub mod bootstrap_dark; pub mod bootstrap_light; pub mod default_dark; pub mod default_light; pub mod palette;

pub trait ThemeSpec {
    fn colors() -> Vec<CssRule>;
    #[must_use] fn render() -> String;          // default: the rules rendered and joined with "\n"
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Theme { DefaultDark, DefaultLight, BootstrapDark, BootstrapLight }
impl Display for Theme;     // "default-dark", "default-light", "bootstrap-dark", "bootstrap-light"
impl Theme {
    pub const ALL: [Self; 4];                   // declaration order
    pub fn iter() -> impl Iterator<Item = Self>;
    #[must_use] pub fn theme(theme: Self) -> String;     // that theme's CSS
    #[must_use] pub const fn palette(self) -> Palette;
}
#[must_use] pub fn theme_shared() -> String;
```

* `Theme::theme(t)`: `:root { --bs-... }` plus `.color-green`, `.color-yellow`, `.color-red` (see [Internals](internals.md#theme-output)).
* `theme_shared()`: the shared stylesheet: `root()`, `header()`, `content()`, `footer()`, `elements()`, `modal()` rules rendered and joined with `"\n"`.
* Each theme module has a unit struct and a constant, all `pub`:

```rust
pub struct DefaultDarkTheme;   impl DefaultDarkTheme   { pub const PALETTE: Palette; }  impl ThemeSpec for DefaultDarkTheme
pub struct DefaultLightTheme;  impl DefaultLightTheme  { pub const PALETTE: Palette; }  impl ThemeSpec for DefaultLightTheme
pub struct BootstrapDarkTheme; impl BootstrapDarkTheme { pub const PALETTE: Palette; }  impl ThemeSpec for BootstrapDarkTheme
pub struct BootstrapLightTheme;impl BootstrapLightTheme{ pub const PALETTE: Palette; }  impl ThemeSpec for BootstrapLightTheme
```
(`#[derive(Debug)]` on each.)

### `theme::palette`

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)] pub struct Color(u32);       // sRGB, Display = "#rrggbb" (lower case)
impl Color {
    #[must_use] pub const fn rgb(value: u32) -> Self;                       // 0xRRGGBB (the value is stored as given)
    #[must_use] pub const fn hex(value: &str) -> Self;                      // "#rrggbb"; panics otherwise
    #[must_use] pub fn mix(first: Self, second: Self, share: f64) -> Self;
    #[must_use] pub fn luminance(self) -> f64;
    #[must_use] pub fn contrast(self, other: Self) -> f64;
    #[must_use] pub const fn red(self) -> u8;
    #[must_use] pub const fn green(self) -> u8;
    #[must_use] pub const fn blue(self) -> u8;
}
pub const GRAY_STEPS: [u16; 11] = [50, 100, 200, 300, 400, 500, 600, 700, 800, 900, 950];
pub const SUCCESS_STEPS: [u16; 5] = [500, 600, 700, 800, 900];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Palette { pub warning: Color, pub success: [Color; 5], pub danger: Color, pub code_bg: Color, pub gray: [Color; 11] }
impl Palette {
    #[must_use] pub const fn new(warning: &str, success: [&str; 5], danger: &str, code_bg: &str, gray: [&str; 11]) -> Self;
    #[must_use] pub fn tokens(&self) -> Vec<(String, Color)>;
    #[must_use] pub fn color(&self, name: &str) -> Option<Color>;
    #[must_use] pub fn rules(&self) -> Vec<CssRule>;
}
```

* `hex`: only exactly seven bytes starting with `#` and six hex digits (either case) are accepted; anything else panics ("expected #rrggbb" / "expected hex digits in #rrggbb"). In a `const` this is a compile error.
* `mix(first, second, share)`: per channel `round(a * share + b * (1 - share))` in sRGB (`share` is the weight of `first`).
* `luminance`: WCAG 2.x relative luminance (threshold `0.03928`, weights 0.2126, 0.7152, 0.0722). `contrast`: `(L1 + 0.05) / (L2 + 0.05)` with the lighter first, 1 to 21.
* `Palette::new`: all values go through `Color::hex`.
* `tokens`: `(--bs-warning, ...)` in emission order: warning, `--bs-success-{500..900}`, danger, `--bs-code-bg`, `--bs-gray-{50..950}` (19 tokens).
* `color(name)`: a token name (`--bs-gray-100`), the literal `white`, or `mix:<token>:<token>:<percent>` (nested mixes allowed; percent parsed as `f64`/100). `None` for unknown names or malformed mixes.
* `rules`: `:root` with the 19 tokens, then `.color-green { color: var(--bs-success-700) }`, `.color-yellow { color: var(--bs-warning) }`, `.color-red { color: var(--bs-danger) }`.

## `styles`

```rust
pub mod common;   // styles::common re-exports, via glob, of:
#[must_use] pub fn root() -> Vec<CssRule>;       // styles::common::root
#[must_use] pub fn header() -> Vec<CssRule>;     // styles::common::header   (layout.rs)
#[must_use] pub fn content() -> Vec<CssRule>;    // styles::common::content  (layout.rs)
#[must_use] pub fn footer() -> Vec<CssRule>;     // styles::common::footer   (layout.rs)
#[must_use] pub fn elements() -> Vec<CssRule>;   // styles::common::elements
#[must_use] pub fn modal() -> Vec<CssRule>;      // styles::common::modal
```

Public submodules: `styles::common::{elements, layout, modal, root}`. Class vocabulary in [Internals](internals.md#shared-stylesheet).

## `tokens`

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq)] pub struct Token<T> { pub name: &'static str, pub value: T }
impl<T: Display> Token<T> { #[must_use] pub fn css(&self) -> String; }          // value.to_string()

#[derive(Copy, Clone, Debug, PartialEq)] pub enum Length { Rem(f32), Px(f32) }
impl Length { #[must_use] pub const fn px(self) -> f32; }                         // 1rem = 16px
impl Display for Length;                                                          // "0.25rem", "9999px"
#[derive(Copy, Clone, Debug, PartialEq)] pub struct Number(pub f32);              // Display: "1.2"
#[derive(Copy, Clone, Debug, PartialEq, Eq)] pub struct Millis(pub u32);          // Display: "120ms"

pub const NARROW_VIEWPORT_PX: u32 = 768;
pub const SPACING: [Token<Length>; 8];
pub const RADII: [Token<Length>; 4];
pub const TEXT_SIZES: [Token<Length>; 7];
pub const LINE_HEIGHTS: [Token<Number>; 2];
pub const ELEVATION: [Token<&str>; 4];
pub const DURATIONS: [Token<Millis>; 3];
pub const REDUCED_MOTION: Millis;                 // Millis(1)
pub const EASING: [Token<&str>; 2];
pub const TOUCH_TARGET: Token<Length>;            // --q-touch-target: 44px
pub const POINTER_TARGET: Token<Length>;          // --q-pointer-target: 32px

#[must_use] pub fn all() -> Vec<(&'static str, String)>;
#[must_use] pub fn rules() -> Vec<CssRule>;
#[must_use] pub fn stylesheet() -> String;
```

Values are listed in [Configuration](configuration.md#design-tokens). `all()` order: spacing, radii, text sizes, line heights, elevation, easing, durations, touch target, pointer target. `rules()` returns the `:root` rule with every token and `@media (prefers-reduced-motion: reduce) { :root { --q-dur-fast: 1ms; --q-dur-base: 1ms; --q-dur-slow: 1ms } }`. (`Millis` and `Number` print through `Display` as shown.)

## `mobile`

```rust
#[must_use] pub fn rules() -> Vec<CssRule>;
#[must_use] pub fn stylesheet() -> String;     // rules rendered, joined with "\n"
```

Classes (namespace `q-m-*`): `q-m-safe`, `q-m-safe-top`, `q-m-safe-bottom`, `q-m-safe-x` (padding from `env(safe-area-inset-*)`); `q-m-touch` (min size `var(--q-touch-target)`, inline-flex centred); `q-m-tap` (no tap highlight, `touch-action: manipulation`); `q-m-select-none`; `q-m-scroll` (`overflow-y: auto`, momentum, `overscroll-behavior: contain`); `q-m-no-bounce`; `q-m-viewport` (`height: 100vh`, with `@supports (height: 100dvh)` -> `100dvh`); `q-m-motion` (transitions on `transform, opacity, background-color` using `--q-dur-base` and `--q-ease-standard`), and a `@media (prefers-reduced-motion: reduce)` block that removes `q-m-motion` transitions and animations.

## `desktop`

```rust
pub const MEDIA: &str = "@media (hover: hover) and (pointer: fine)";
#[must_use] pub fn rules() -> Vec<CssRule>;     // one rule: MEDIA with the children below
#[must_use] pub fn stylesheet() -> String;
```

Classes (all inside `MEDIA`): `q-d-hover:hover` (background `var(--bs-gray-800)`, `cursor: pointer`); `q-d-focus:focus-visible` (2px outline `var(--bs-success-500)`, offset 2px); `q-d-dense` (padding `var(--q-space-2) var(--q-space-3)`, `font-size: var(--q-text-sm)`, `min-height: var(--q-pointer-target)`); `q-d-scroll` (thin scrollbar, `scrollbar-color: var(--bs-gray-600) transparent`); `q-d-selectable` (`user-select: text`).

## `components`

```rust
pub const TEXT_PAIRS: &[(&str, &str)];           // (text, surface) palette tokens; each must reach 4.5:1
pub const DANGER_TEXT_TOKEN: &str;               // "mix:--bs-danger:--bs-gray-100:60"
pub const EDGE_PAIRS: &[(&str, &str)];           // control edges / focus rings; each must reach 3:1
#[must_use] pub fn rules() -> Vec<CssRule>;
#[must_use] pub fn stylesheet() -> String;
```

`TEXT_PAIRS` = gray-100 on gray-950/900/800, gray-300 on gray-950/900, `white` on success-800, and `DANGER_TEXT_TOKEN` on gray-950/900. `EDGE_PAIRS` = gray-400 and gray-100 on gray-950 and gray-900 (see the test in [Testing](testing.md)). Class list in [Internals](internals.md#component-classes).

## `ftl`

```rust
#[derive(Debug, Clone, PartialEq, Eq)] pub struct Message { pub id: String, pub value: String }
#[derive(Debug, Clone, PartialEq, Eq)] pub struct FtlError { pub line: usize, pub message: String }   // Display: "line {line}: {message}"; std::error::Error
pub fn parse(source: &str) -> Result<Vec<Message>, FtlError>;
```

Parses the Fluent subset described in [Internals](internals.md#fluent-subset). `line` is 1-based.

## `i18n`

```rust
pub fn available_locales() -> Result<Vec<String>>;                                   // ./i18n
pub fn available_locales_in(dir: &Path) -> Result<Vec<String>>;
pub fn validate_locales_exist(locales: &[String]) -> Result<()>;                      // ./i18n
pub fn validate_locales_exist_in(dir: &Path, locales: &[String]) -> Result<()>;
pub fn parse_ftl_with_options(supported_locales: Option<&[String]>) -> Result<String>;  // ./i18n
pub fn parse_ftl_in(dir: &Path, supported_locales: Option<&[String]>) -> Result<String>;
#[must_use] pub fn generate_translations_js(supported_locales: &[String]) -> String;
```

`Result` is `quench_core::Result`. The directory name is the constant `i18n`, relative to the working directory.

* `available_locales_in`: file stems of `*.ftl` files in `dir`, sorted and de-duplicated; `[]` when `dir` does not exist; `Err` when it cannot be read.
* `validate_locales_exist_in`: `Err("missing locale file: {dir}/{locale}.ftl")` for the first missing file.
* `parse_ftl_in`: `"{}"` when `dir` does not exist. Otherwise pretty JSON `{ "<locale>": { "<id>": "<text>" } }` with locales and ids sorted (BTreeMap), restricted to `supported_locales` when `Some`. A parse error is `Err` with context `Failed to parse FTL: <path>`.
* `generate_translations_js`: `"window.qTranslations = {json};"`; if parsing fails the JSON part is empty (`window.qTranslations = ;`).

## `assets`

```rust
pub const FAVICON: &[u8];            // include_bytes!("../favicon.png")
pub fn create_asset_files(default_theme: Theme, resources_prefix: &str);
pub fn create_asset_files_with_options(_default_theme: Theme, supported_themes: &[Theme], supported_locales: &[String], _resources_prefix: &str);
pub fn write_assets(root: &Path, supported_themes: &[Theme], supported_locales: &[String]);
```

* `create_asset_files`: all four themes, all locales found in `./i18n`, written under `dist/assets` (relative to the working directory). **`default_theme` and `resources_prefix` are ignored** (the underscore parameters of the `_with_options` variant are unused).
* `create_asset_files_with_options`: the same with explicit lists.
* `write_assets`: see [Usage](usage.md#3-write-the-asset-files-for-a-server). Creates `css/themes` and `js` directories; writes `css/style.css`, `favicon.png`, `css/themes/<theme>.css` per theme and `js/translations.js`. Every failure is reported with `eprintln!("ERROR: ...")` and does not stop the others.
