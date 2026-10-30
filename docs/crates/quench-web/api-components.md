---
title: Components and DOM helpers
parent: quench-web
grand_parent: Crates
nav_order: 3
---

Reference for `quench_web::framework::components` and `quench_web::framework::dom` (source: `src/framework/components/`, `src/framework/dom/`). Both modules are re-exported by `quench_web::framework`, by the crate root (`quench_web::HeaderBuilder`, `quench_web::dom::on_dom_ready`, ...), and by [quench-web-components](../quench-web-components/api-components.md). Only `locale_switch` of this group is in `quench_web::prelude`.

## `components::header`

```rust
#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder { /* label, with_nav, nav_panel */ }
```

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | empty label, no nav |
| `label` | `#[must_use] pub fn label(self, value: impl Into<String>) -> Self` | the i18n key (see below) |
| `with_nav` | `#[must_use] pub fn with_nav(self, value: Element) -> Self` | stores the navigation panel and turns the nav button on |
| `build` | `pub fn build(self) -> Element` | not `#[must_use]` |

`build` output:

```html
<header class="q-shell-header">
  <div class="left-panel q-shell-header-left">
    <nav class="q-shell-nav-trigger" onclick="..."><i class="fas fa-grip"></i></nav>   <!-- only with_nav -->
    <h2 data-i18n="{label}"></h2>
    {nav panel}                                                                           <!-- only with_nav -->
  </div>
</header>
```

The heading has **no text**: the label is an i18n key written into `data-i18n`, and the text appears when the locale script runs (so a key that is missing from the catalog leaves an empty heading).

## `components::footer`

```rust
#[derive(Clone, Debug, Default)]
pub struct FooterBuilder { /* label */ }
```

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new() -> Self` |
| `label` | `#[must_use] pub fn label(self, value: impl Into<String>) -> Self` (an i18n key) |
| `build` | `#[must_use] pub fn build(self) -> Element` |

Output: `<footer class="footer q-shell-footer"><p data-i18n="{label}"></p></footer>`.

## `components::nav`

### `nav_button`

```rust
#[must_use]
pub fn nav_button() -> Element
```

`<nav class="q-shell-nav-trigger" onclick="...">` containing `<i class="fas fa-grip"></i>`. The click handler is `toggle_modal("modal-overlay", "modal-side", "show")`.

### `locale_switch`

```rust
#[must_use]
pub fn locale_switch(
    supported_locales: Option<Vec<String>>,
    default_locale: Option<String>,
) -> Element
```

A standalone language `<select>`: `<div class="locale-switch q-shell-locale-switch">` with the select (`id="locale-select"`, `onchange` calling `updateLocale`) and a deferred script that sets the select's value from `getLocale()` once the DOM is ready. Locales: the argument, or `available_locales()` or `["en-US"]` if that fails. Default: `default_locale` when it is in the list, else the first locale, else `en-US`. Each option's value is the locale and its text is `"{flag} {locale}"`, where the flag is the regional-indicator pair for the part after the first `-` or `_` when that is two ASCII letters (`pl-PL` gives the Polish flag emoji), otherwise empty (the text is trimmed).

The `value` attribute written on the `<select>` is cosmetic (a `select` has no `value` attribute); the script is what selects the option.

### `NavPanelBuilder`

```rust
#[derive(Clone, Debug, Default)]
pub struct NavPanelBuilder {
    pub default_theme: Option<Theme>,
    pub default_locale: Option<String>,
    pub supported_themes: Option<Vec<Theme>>,
    pub supported_locales: Option<Vec<String>>,
}
```

The slide-out settings panel (language and theme selects).

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new() -> Self` |
| `default_theme` | `#[must_use] pub const fn default_theme(self, default_theme: Theme) -> Self` |
| `default_locale` | `#[must_use] pub fn default_locale(self, default_locale: impl Into<String>) -> Self` |
| `supported_themes` | `#[must_use] pub fn supported_themes(self, supported_themes: Vec<Theme>) -> Self` |
| `supported_locales` | `#[must_use] pub fn supported_locales(self, supported_locales: Vec<String>) -> Self` |
| `build` | `#[must_use] pub fn build(self) -> Element` |

`build` output (a `div` without a class):

```html
<div>
  <div class="modal-overlay" onclick="{toggle}"></div>
  <div class="modal-side q-shell-modal-side">
    <div class="modal-content q-shell-modal-content">
      <label data-i18n="locale_label"></label>
      <select id="locale-select" ...>...</select>
      <label data-i18n="theme_label"></label>
      <select id="theme-select" onchange="...updateTheme..." value="{default}">
        <option value="default-dark">default-dark</option> ...
      </select>
    </div>
  </div>
  <script defer>{on DOMContentLoaded: set #locale-select from getLocale(), #theme-select from getTheme()}</script>
</div>
```

The panel expects the translation keys `locale_label` and `theme_label` in the catalogs. Themes default to every theme; the theme `<select>`'s option text is the theme name (`default-dark`, ...), not localized.

## `dom`

JavaScript snippet generators. They return `String`s to be embedded in `onclick`/`onchange` attributes or `<script>` bodies (use `script(..).raw()`). Arguments are interpolated into single-quoted JS strings **without escaping**: pass only trusted, constant identifiers.

### `dom::events::on_dom_ready`

```rust
#[must_use]
pub fn on_dom_ready(blocks: &[String]) -> String
```

Returns `document.addEventListener("DOMContentLoaded", () => {\n{blocks joined by "\n"}\n});`.

### `dom::modal::toggle_modal`

```rust
#[must_use]
pub fn toggle_modal(overlay_class: &str, panel_class: &str, show_class: &str) -> String
```

Returns JS that looks up elements by class name and, only when there is **exactly one** overlay and at least one panel, toggles `show_class` on the first of each. It declares `const overlay` and `const panel`, so it must not be combined with other snippets that declare the same names in one scope.

### `dom::select::update_from_select`

```rust
#[must_use]
pub fn update_from_select(select_id: &str, update_fn: &str) -> String
```

Returns `const selected = document.getElementById('{select_id}').value;{update_fn}(selected);`.

### `dom::select::set_select_value`

```rust
#[must_use]
pub fn set_select_value(select_id: &str, get_fn: &str) -> String
```

Returns JS that sets the `<select>` with that id (if it exists) to `{get_fn}()`.

```rust
use quench_web::{dom::*, prelude::*};

let init = script(on_dom_ready(&[set_select_value("theme-select", "getTheme")])).raw().defer();
```
