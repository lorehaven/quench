---
title: quench-web
parent: Crates
nav_order: 60
has_children: true
permalink: /crates/quench-web/
---

`quench-web` builds server-rendered HTML pages from Rust values: a chainable `Element` builder, a `PageBuilder` for whole documents, and an app shell (header, navigation panel, footer, theme and locale switching, session watcher) shared by the estate's services. It replaces a template engine for pages that are assembled in code.

| | |
|---|---|
| Crate / library name | `quench-web` / `quench_web` |
| Version | 0.1.25 |
| Edition | 2024 (workspace) |
| Custom test harness | `harness = false` (`quench_test::main!()`) |
| Unsafe code | none |

## Install

```toml
[dependencies]
quench-web = { version = "0.1", registry = "ennor" }
```

Inside the workspace use `quench-web = { workspace = true }`.

## Feature flags

None. The crate declares no `[features]`.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-core` | `quench_core::Result` and `bail!` for `AppShellBuilder::try_build` and the locale checks. |
| `quench-style` | Themes, base CSS, `CssRule`, asset writing and i18n catalogs. `quench-web` re-exports them at their old paths. |
| `quench-test` (dev) | Test harness and `#[quench_test::test]`. |

No third-party crates.

## What is in the crate

| Module | Content | Page |
|---|---|---|
| `html::builder::element` | `Element` | [HTML builders](api-html.md) |
| `html::builder::elements` | 28 tag helper functions | [HTML builders](api-html.md) |
| `html::builder::page` | `Link`, `Script`, `js!`, `PageBuilder`, `pretty_print_html` | [HTML builders](api-html.md) |
| `framework::app` | `AppBuilder`, `AppShellBuilder`, `AppShell`, `locale_script`, `session_script`, `theme_script` | [App shell](api-app.md) |
| `framework::components` | `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder`, `nav_button`, `locale_switch` | [Components and DOM helpers](api-components.md) |
| `framework::dom` | `on_dom_ready`, `toggle_modal`, `update_from_select`, `set_select_value` | [Components and DOM helpers](api-components.md) |
| `styling::css`, `framework::{styles, theme}`, asset and i18n functions | re-exports of `quench-style` | [Re-exports](api-reexports.md) |

## Where to go next

* [Usage](usage.md): worked examples from a single element to a full shell.
* [App shell](api-app.md): what `AppShellBuilder` and `AppBuilder` put on a page.
* [Configuration](configuration.md): builder options, the `i18n/` directory, the cookies and the files written to `dist/assets`.
* [Errors and panics](errors.md).
* [Internals](internals.md): rendering rules, escaping, the pretty printer and the embedded scripts.
* [Testing](testing.md).
* [quench-web-components](../quench-web-components/index.md): buttons, cards, forms and status widgets built on `Element`.

## Minimal example

```rust
use quench_web::prelude::*;

fn main() {
    let shell = AppShellBuilder::new()
        .title("Demo")
        .default_theme(Theme::DefaultDark)
        .build(); // writes dist/assets/... into the working directory

    let html = shell.page(
        div()
            .class("container")
            .child(h1().text("Hello from Quench"))
            .child(p().text("Rendered from Element builders")),
    );
    println!("{html}");
}
```

`AppShellBuilder::build` creates `dist/assets/` relative to the working directory (see [Configuration](configuration.md#files-written-to-distassets)). The service that embeds the page must serve that directory under `{resources_prefix}/assets/`.
