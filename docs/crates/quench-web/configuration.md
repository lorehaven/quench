---
title: Configuration
parent: quench-web
grand_parent: Crates
nav_order: 6
---

`quench-web` reads no environment variables and has no configuration file. Behaviour is set through builder calls, the working directory, and the `i18n/` directory.

## Builder options

See [App shell](api-app.md) for every option with its default. The ones that change on-disk or URL behaviour:

| Option | Where | Effect |
|---|---|---|
| `resources_prefix` | `AppBuilder`, `AppShellBuilder` | URL prefix of the generated asset links (`{P}/assets/...`), of the theme map in `theme_script`, and of the session endpoints `{P}/status`, `{P}/refresh`, `{P}/login`. It is **not** used when writing files: assets are always written to `dist/assets`. |
| `supported_themes` | both | Themes offered in the panel, preloaded and written to disk. |
| `supported_locales` | both | Locales offered, included in `translations.js`, validated against `i18n/` by `AppShellBuilder`. |
| `default_theme`, `default_locale` | both | Fallbacks used when the `qtheme` / `qlocale` cookie is missing or unknown. `AppShellBuilder` corrects an unsupported default (first supported value wins). |

## Working directory

Two relative paths are resolved against the process working directory, not the binary location:

| Path | Used for |
|---|---|
| `./i18n/` | `*.ftl` catalogs: `available_locales()`, `validate_locales_exist()`, `generate_translations_js()` |
| `./dist/assets/` | output of `create_asset_files*` and `AppShellBuilder::try_build` |

## Files written to `dist/assets`

Written by `AppShellBuilder::try_build` (every time it runs), `create_asset_files` and `create_asset_files_with_options`:

| File | Content |
|---|---|
| `dist/assets/css/style.css` | `theme_shared()` |
| `dist/assets/css/themes/{theme}.css` | one per supported theme (`default-dark.css`, `default-light.css`, `bootstrap-dark.css`, `bootstrap-light.css`) |
| `dist/assets/favicon.png` | the embedded `FAVICON` |
| `dist/assets/js/translations.js` | `window.qTranslations = {...};` for the supported locales |

The directories are created on demand. Errors are printed to stderr as `ERROR: failed to ...` and do not abort. A service must serve this directory itself, mapping `{resources_prefix}/assets/` to it.

## The `i18n/*.ftl` format

One file per locale, named `{locale}.ftl` (the stem is the locale code, e.g. `en-US.ftl`). The shell uses the keys:

| Key | Used by |
|---|---|
| `header_label` | default header title (changeable through `header_label(..)`) |
| `footer_label` | default footer text (changeable through `footer_label(..)`) |
| `locale_label` | label of the language select in the panel |
| `theme_label` | label of the theme select in the panel |

Messages may contain `{$name}` placeholders, filled in the browser from the element's `data-i18n-args` JSON. The parser is `quench_style::ftl` (a subset of Fluent; see the `quench-style` documentation).

## Cookies set by the page scripts

| Cookie | Written by | Values | Lifetime |
|---|---|---|---|
| `qlocale` | `locale_script` | a locale code | 365 days, `path=/` |
| `qtheme` | `theme_script` | `default-dark`, `default-light`, `bootstrap-dark`, `bootstrap-light` | 365 days, `path=/` |

They carry no `Secure`, `SameSite` or `HttpOnly` attributes (set from JavaScript).

## Constants

| Name | Value | Where |
|---|---|---|
| `SESSION_CHECK_SECS` | `60` | `framework/app/builder.rs`, private; session poll interval |
| Font Awesome | `6.7.2` from cdnjs | `FONTAWESOME_CSS` (private) |
| htmx | `2.0.10` from jsDelivr | `HTMX_SCRIPT` (private) |
| htmx SSE extension | `2.2.4` from jsDelivr | `HTMX_SSE_SCRIPT` (private) |
