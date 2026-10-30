---
title: Errors and panics
parent: quench-web
grand_parent: Crates
nav_order: 7
---

The crate has no error type of its own. Fallible functions return `quench_core::Result`; the rest panic or swallow errors as listed here.

## Returned errors

| Function | Error | When |
|---|---|---|
| `AppShellBuilder::try_build` | `missing locale file: i18n/{locale}.ftl` | a locale given to `supported_locales(..)` has no `.ftl` file |
| `available_locales`, `available_locales_in` | I/O error | the directory exists but cannot be read |
| `validate_locales_exist*` | `missing locale file: {path}` | a file is missing |
| `parse_ftl_with_options`, `parse_ftl_in` | `Failed to parse FTL: {path}` (with cause) or I/O error | unreadable or invalid `.ftl` |

## Panics

| Function | Message | Cause |
|---|---|---|
| `PageBuilder::build` | `called `Option::unwrap()` on a `None` value` | `.content(..)` was never called |
| `AppShellBuilder::build` | `failed to build app shell: {err}` | `try_build` returned an error |

## Swallowed errors

| Place | What happens |
|---|---|
| `create_asset_files*`, `write_assets` | file-system errors are printed to stderr (`ERROR: failed to ...`); the call returns normally |
| `generate_translations_js` | a parse or read error yields `window.qTranslations = ;` (a JavaScript syntax error in the browser) |
| `AppShellBuilder` with no `supported_locales` | an `available_locales()` error becomes an empty list |
| `AppBuilder::build` with no `supported_locales` | same |
| `build_locale_select` (in `locale_switch`, `NavPanelBuilder`) | an `available_locales()` error becomes `["en-US"]` |
