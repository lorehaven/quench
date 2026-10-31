# quench-web
A simple web UI framework: server-rendered HTML pages built from Rust values, plus a shared app shell.

## What it is
`quench-web` builds server-rendered HTML pages from Rust values: a chainable `Element` builder, a `PageBuilder` for whole documents, and an app shell (header, navigation panel, footer, theme and locale switching, session watcher) shared by the estate's services. It replaces a template engine for pages that are assembled in code.

It builds on `quench-core` and `quench-style` (themes, base CSS, i18n catalogs, re-exported here). Widgets built on `Element` live in `quench-web-components`.

## Use cases
- Render HTML from code with escaped text and attributes.
- Produce a complete pretty-printed document with `PageBuilder`.
- Give a service a consistent page with header, settings panel and footer through `AppShellBuilder`.
- Switch themes and locales using `i18n/<locale>.ftl` catalogs.

## Example
```rust
use quench_web::prelude::*;

let html = ul()
    .class("list")
    .child(li().text("one"))
    .child(li().text("two & three"))
    .render();
assert_eq!(html, r#"<ul class="list"><li>one</li><li>two &amp; three</li></ul>"#);
```

A service page inside the shell:

```rust
use quench_web::prelude::*;
use std::sync::Arc;

fn make_shell() -> Arc<AppShell> {
    Arc::new(
        AppShellBuilder::new()
            .title("Switchboard")
            .header_label("header_label")
            .footer_label("footer_label")
            .resources_prefix("/switchboard/ui".to_string())
            .build(),
    )
}

fn index(shell: &AppShell) -> String {
    shell.page(div().child(h2().text("Models")))
}
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-web/
