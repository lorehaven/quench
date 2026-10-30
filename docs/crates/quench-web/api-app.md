---
title: App shell
parent: quench-web
grand_parent: Crates
nav_order: 2
---

Reference for `quench_web::framework::app` (source: `src/framework/app/`). All items are re-exported by `quench_web::framework`, by the crate root, and (`AppBuilder`, `AppShell`, `AppShellBuilder`) by `quench_web::prelude`.

## `AppBuilder`

```rust
#[derive(Clone, Debug)]
pub struct AppBuilder { /* all fields pub(crate) */ }
impl Default for AppBuilder
```

The low-level page assembler: it renders a full document with the standard head (icon, Font Awesome, base CSS, theme CSS, htmx, translation and session scripts) around a `div.app` that holds an optional header, the content, and an optional footer. It does **not** write asset files and does not create a header or footer for you; [`AppShellBuilder`](#appshellbuilder) does.

| Method | Signature | Default / behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | `supported_themes` = all four themes, `default_theme` = `Theme::DefaultDark`, everything else empty. (`Default::default()` differs: `supported_themes` is empty.) |
| `title` | `pub fn title(self, value: impl Into<String>) -> Self` | `""` |
| `links` | `pub fn links(self, value: Vec<Link>) -> Self` | extra `<link>`s, appended after the standard ones |
| `scripts` | `pub fn scripts(self, value: Vec<Script>) -> Self` | extra scripts, appended after the standard ones |
| `supported_themes` | `pub fn supported_themes(self, value: Vec<Theme>) -> Self` | theme list used for the theme script and for preloads |
| `supported_locales` | `pub fn supported_locales(self, value: Vec<String>) -> Self` | when empty at `build`, `available_locales()` is used (`unwrap_or_default`) |
| `default_theme` | `pub const fn default_theme(self, value: Theme) -> Self` | `Theme::DefaultDark`; not checked against `supported_themes` |
| `default_locale` | `pub fn default_locale(self, value: impl Into<String>) -> Self` | `None`; if unset or not in the locale list the first locale (else `en-US`) is used |
| `header` | `pub fn header(self, value: Element) -> Self` | none |
| `page_content` | `pub fn page_content(self, value: Element) -> Self` | none (an empty content area) |
| `footer` | `pub fn footer(self, value: Element) -> Self` | none |
| `resources_prefix` | `pub fn resources_prefix(self, value: String) -> Self` | `""`; prefixed to every asset URL and to the session endpoints |
| `build` | `pub fn build(self) -> String` | renders the page |

All setters are `#[must_use]`, as is `build`.

### What `build` emits

Head, in order (`PageBuilder` places all links first, then all scripts, then `<title>`):

Links:

1. `<link rel="icon" href="{P}/assets/favicon.png">`
2. `<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.7.2/css/all.min.css">`
3. `<link rel="stylesheet" href="{P}/assets/css/style.css">`
4. `<link rel="stylesheet" id="theme-link" href="{P}/assets/css/themes/{default_theme}.css">`
5. for every supported theme other than the default: `<link rel="preload" as="style" href="{P}/assets/css/themes/{theme}.css">`
6. the links passed to `.links(..)`

Scripts:

1. inline `window.global = window.global || window; window.global.global = window.global;`
2. `https://cdn.jsdelivr.net/npm/htmx.org@2.0.10/dist/htmx.min.js` (no `defer`)
3. `https://cdn.jsdelivr.net/npm/htmx-ext-sse@2.2.4/dist/sse.min.js` (no `defer`)
4. `{P}/assets/js/translations.js` (no `defer`)
5. `locale_script(..)`, 6. `theme_script(..)`, 7. `session_script(P, 60)` (inline)
8. the scripts passed to `.scripts(..)`

`{P}` is `resources_prefix`. Body:

```html
<div class="app q-shell-app">
  {header}
  <div class="content q-shell-content">
    <div class="content-inner q-shell-content-inner">{content}</div>
  </div>
  {footer}
</div>
```

The Font Awesome stylesheet and both htmx scripts are loaded from public CDNs, so pages need network access to those hosts to be fully functional.

The session watcher interval is the private constant `SESSION_CHECK_SECS = 60`; it is not configurable.

## `AppShellBuilder`

```rust
#[derive(Clone, Debug)]
pub struct AppShellBuilder { /* all fields pub(crate) */ }
impl Default for AppShellBuilder
```

The opinionated builder most services use. It resolves themes and locales, writes the static assets to disk, creates the default header (with navigation panel) and footer, and returns an [`AppShell`](#appshell).

| Method | Signature | Default |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | same as `default()` |
| `title` | `pub fn title(self, value: impl Into<String>) -> Self` | `"Quench"` |
| `default_theme` | `pub const fn default_theme(self, value: Theme) -> Self` | `Theme::DefaultDark` |
| `default_locale` | `pub fn default_locale(self, value: impl Into<String>) -> Self` | `None` |
| `header_label` | `pub fn header_label(self, value: impl Into<String>) -> Self` | `"header_label"` (an i18n key, see below) |
| `footer_label` | `pub fn footer_label(self, value: impl Into<String>) -> Self` | `"footer_label"` |
| `with_nav` | `pub const fn with_nav(self, value: bool) -> Self` | `true` |
| `with_header` | `pub const fn with_header(self, value: bool) -> Self` | `true`. `false` drops the top bar entirely (for pages with their own chrome, e.g. a login card). |
| `header` | `pub fn header(self, value: Element) -> Self` | `None`: use the generated header. A given header is used as is; `with_nav` and `header_label` are then ignored. |
| `footer` | `pub fn footer(self, value: Element) -> Self` | `None`: use the generated footer. |
| `links` | `pub fn links(self, value: Vec<Link>) -> Self` | empty |
| `scripts` | `pub fn scripts(self, value: Vec<Script>) -> Self` | empty |
| `supported_themes` | `pub fn supported_themes(self, value: Vec<Theme>) -> Self` | `None` = all four |
| `supported_locales` | `pub fn supported_locales(self, value: Vec<String>) -> Self` | `None` = every `i18n/*.ftl` found |
| `resources_prefix` | `pub fn resources_prefix(self, value: String) -> Self` | `""` |
| `try_build` | `pub fn try_build(self) -> quench_core::Result<AppShell>` | see below |
| `build` | `#[must_use] pub fn build(self) -> AppShell` | `try_build` and panic on error |

All setters except `new`/`try_build`/`build` are `#[must_use]`.

`try_build` does, in order:

1. **Themes.** `supported_themes` (or all themes if `None`, or all themes if the given list is empty). The effective default theme is `default_theme` if it is in that list, else the first element of the list.
2. **Locales.** If `supported_locales` was given, each locale must have `i18n/<locale>.ftl` (`validate_locales_exist`; `Err("missing locale file: i18n/<locale>.ftl")` otherwise). If not given, `available_locales().unwrap_or_default()` (an unreadable or missing `i18n` directory gives an empty list). The effective default locale is `default_locale` if it is in the list, else the first locale, else none.
3. **Assets.** Calls `create_asset_files_with_options(..)`, which writes `dist/assets/...` in the working directory (see [Configuration](configuration.md#files-written-to-distassets)). Write failures are printed to stderr and do not fail the build.
4. **Header** (only if `with_header`): the given `header`, or `HeaderBuilder::new().label(header_label)` and, if `with_nav`, `.with_nav(NavPanelBuilder ...)` configured with the effective theme, locale and lists.
5. **Footer:** the given `footer`, or `FooterBuilder::new().label(footer_label).build()`.
6. Assembles an `AppBuilder` with all of the above (the effective default locale, or `""` when there is none) and returns `AppShell { base }`.

```rust
use quench_web::prelude::*;

let shell = AppShellBuilder::new()
    .title("Switchboard")
    .supported_themes(vec![Theme::BootstrapDark, Theme::BootstrapLight])
    .default_theme(Theme::DefaultDark) // not supported: BootstrapDark is used instead
    .resources_prefix("/switchboard/ui".to_string())
    .try_build()?;
```

## `AppShell`

```rust
#[derive(Clone, Debug)]
pub struct AppShell { /* base: AppBuilder */ }

impl AppShell {
    #[must_use]
    pub fn page(&self, content: Element) -> String
}
```

`page` clones the stored `AppBuilder`, sets `page_content(content)` and calls `build()`. Cheap enough to call on every request; `AppShell` is `Clone`, and contains only owned data, so it is `Send + Sync` and can be stored in shared application state.

## Script generators

Free functions returning inline `Script`s with an embedded JavaScript body. All are `#[must_use]`.

### `locale_script`

```rust
pub fn locale_script(supported_locales: &[String], default_locale: Option<&str>) -> Script
```

Defines, in the page's global scope: constants `DEFAULT_LOCALE` and `COOKIE_NAME = "qlocale"`; helpers `getCookie(name)` and `setCookie(name, value, days = 365)` (path `/`); `getLocale()`, `translate(key, fallback)`, `applyTranslations(locale)`, `updateLocale(newLocale)`, `watchLocaleChanges()`. Exposed on `window`: `qUpdateI18n()`, `qT(key, fallback)`, `setLocale(locale)`, `getLocale()`. The default locale is `default_locale` if it is in `supported_locales`, else the first supported locale, else `"en-US"`.

Behaviour in the browser:

* `getLocale()` reads the `qlocale` cookie; if it is missing or has no entry in `window.qTranslations`, the default is stored in the cookie and returned.
* `applyTranslations` sets `textContent` of every `[data-i18n]` element, `placeholder` of `[data-i18n-placeholder]`, and `title` of `[data-i18n-title]`, from `window.qTranslations[locale]`. Keys missing from the catalog leave the element untouched. `data-i18n-args` (JSON object) replaces `{$name}` placeholders.
* Runs on `DOMContentLoaded`, after every `htmx:afterSwap`, and every 500 ms when the cookie changes (so other tabs follow). A `MutationObserver` re-applies translations when inserted nodes carry `data-i18n*` attributes (for example SSE fragments).
* `updateLocale(new)` ignores unknown locales; otherwise stores the cookie, applies translations and dispatches `localeChanged` on `window`.

The script reads `window.qTranslations`, which comes from `{P}/assets/js/translations.js`.

### `theme_script`

```rust
pub fn theme_script(default_theme: &str, supported_themes: &[Theme], resources_prefix: &str) -> Script
```

Defines `DEFAULT_THEME`, `THEME_COOKIE = "qtheme"` and a `THEMES` map from theme name (`default-dark`, ...) to `"{resources_prefix}/assets/css/themes/{name}.css"`; `getTheme()`, `applyTheme(theme)` (sets `href` of `#theme-link`, creating the link if absent), `updateTheme(new)` (ignores unknown themes; stores cookie, applies, dispatches `themeChanged`), and a 500 ms cookie watcher. It applies the stored theme immediately when the script runs. **It uses `getCookie` and `setCookie` from `locale_script`, so it must come after it** (`AppBuilder` guarantees this).

### `session_script`

```rust
pub fn session_script(resources_prefix: &str, interval_secs: u64) -> Script
```

A session watcher. URLs are `{prefix}/status`, `{prefix}/refresh`, `{prefix}/login`; the interval is `interval_secs * 1000` ms.

* Every interval, and on `visibilitychange` back to visible, it runs `checkSession()`. It does nothing while the tab is hidden, while a check is in flight, or when the path ends in `/login` (trailing slash ignored).
* It `GET`s the status URL (`credentials: "same-origin"`, `Accept: application/json`, `cache: "no-store"`). A non-2xx response, a network failure or an unparsable body is ignored.
* Only a JSON body with `authenticated === false` counts as evidence. Then it `POST`s the refresh URL; if that is not `ok` (or fails), `window.location.href` is set to the login URL.

So a service using the default shell is expected to serve `GET {prefix}/status` returning `{"authenticated": bool}`, `POST {prefix}/refresh` and `GET {prefix}/login`. A service without `/status` (404) is simply never redirected.

## Re-exports from `quench-style`

`framework::app` also re-exports (`#[doc(inline)]`) everything in `quench_style::assets` and `quench_style::i18n`. They are listed in [Re-exports](api-reexports.md).
