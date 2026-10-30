---
title: Configuration
parent: quench-style
nav_order: 3
---

`quench-style` has no environment variables and no config file. What it reads and writes, and the values you can pick, are below.

## Themes

| `Theme` | `Display` (file name) | Character |
|---|---|---|
| `DefaultDark` | `default-dark` | warm amber warning, emerald success, neutral grays (dark) |
| `DefaultLight` | `default-light` | same hues, gray ramp reversed |
| `BootstrapDark` | `bootstrap-dark` | Bootstrap-like muted colours on dark grays |
| `BootstrapLight` | `bootstrap-light` | Bootstrap-like colours, light grays |

Palette values (`#rrggbb`):

| Token | default-dark | default-light | bootstrap-dark | bootstrap-light |
|---|---|---|---|---|
| `--bs-warning` | f59e0b | f59e0b | ffda6a | ffc107 |
| `--bs-success-500` .. `-900` | 10b981 059669 047857 065f46 064e3b | same | 75b798 5f9f84 4f8a72 3d6f5b 2b4f41 | 198754 157347 146c43 0f5132 0a3622 |
| `--bs-danger` | be123c | be123c | ea868f | dc3545 |
| `--bs-code-bg` | 1f2937 | 1f2937 | 495057 | 343a40 |
| `--bs-gray-50` .. `-950` (11 steps) | fafafa f5f5f5 e5e5e5 d4d4d4 a3a3a3 737373 525252 262626 121212 0d0d0d 050505 | 0a0a0a 171717 262626 404040 525252 737373 a3a3a3 d4d4d4 e5e5e5 f5f5f5 fafafa | f8f9fa f1f3f5 e9ecef dee2e6 ced4da adb5bd 6c757d 495057 343a40 212529 0f1114 | 0f1114 161a1d 1f252b 2b3035 343a40 495057 6c757d adb5bd ced4da dee2e6 f8f9fa |

Every theme defines the same 19 tokens; the stylesheets use `var(--bs-gray-N)` etc., so switching theme means loading a different `css/themes/<name>.css`.

## Design tokens

All published as custom properties by `tokens::stylesheet()`.

| Property | Value |
|---|---|
| `--q-space-1` .. `--q-space-8` | 0.25rem, 0.5rem, 0.75rem, 1rem, 1.5rem, 2rem, 3rem, 4rem |
| `--q-radius-sm` / `-md` / `-lg` / `-full` | 0.2rem, 0.35rem, 0.75rem, 9999px |
| `--q-text-xs` .. `--q-text-3xl` | 0.75, 0.875, 1, 1.25, 1.5, 2, 3 (rem) for `xs sm md lg xl 2xl 3xl` |
| `--q-leading-tight` / `-normal` | 1.2, 1.5 |
| `--q-elevation-0` .. `-3` | `none`; `0 1px 2px rgba(0, 0, 0, 0.3)`; `0 4px 8px rgba(0, 0, 0, 0.35)`; `0 12px 24px rgba(0, 0, 0, 0.4)` |
| `--q-ease-standard` / `--q-ease-exit` | `cubic-bezier(0.2, 0, 0, 1)` / `cubic-bezier(0.4, 0, 1, 1)` |
| `--q-dur-fast` / `-base` / `-slow` | 120ms, 200ms, 320ms (1ms under `prefers-reduced-motion: reduce`) |
| `--q-touch-target` / `--q-pointer-target` | 44px / 32px |
| (Rust only) `NARROW_VIEWPORT_PX` | 768 (the `max-width` of the shared stylesheet's narrow layouts) |

The shell also defines its own custom properties in `:root` (from `styles::common::root`): `--q-shell-font` (`'Roboto', sans-serif`), `--q-shell-panel-radius` (0.35rem), `--q-shell-panel-border`, `--q-shell-panel-shadow`, `--q-shell-panel-bg` (gray-900), `--q-shell-panel-bg-soft` (gray-800), `--q-shell-panel-bg-strong` (gray-950), `--q-shell-text` (gray-300), `--q-shell-text-muted` (gray-500), `--q-shell-accent` (success-700).

## Files read and written

| Path (relative to the working directory) | Direction | By |
|---|---|---|
| `i18n/<locale>.ftl` | read | `i18n::available_locales`, `validate_locales_exist`, `parse_ftl_with_options`, `generate_translations_js`, `assets::create_asset_files*` |
| `dist/assets/css/style.css` | write | `assets::create_asset_files*` (or `write_assets(root, ..)` under `root`) |
| `dist/assets/css/themes/<theme>.css` | write | same |
| `dist/assets/js/translations.js` | write | same |
| `dist/assets/favicon.png` | write | same (bytes of `assets::FAVICON`) |

The `*_in` variants and `write_assets` take an explicit directory instead.

## Locale files

File name = locale = stem (`en-US.ftl`). The JSON object keys are those stems. Supported Fluent syntax is in [Internals](internals.md#fluent-subset).
