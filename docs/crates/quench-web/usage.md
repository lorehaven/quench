---
title: Usage
parent: quench-web
grand_parent: Crates
nav_order: 1
---

Worked examples, from a single element to a complete service page. All use `use quench_web::prelude::*;` unless noted.

## 1. Render an element

Goal: build markup without a template.

```rust
let html = ul()
    .class("list")
    .child(li().text("one"))
    .child(li().text("two & three"))
    .render();
assert_eq!(html, r#"<ul class="list"><li>one</li><li>two &amp; three</li></ul>"#);
```

Text and attribute values are escaped. Pitfalls: `class()` appends while `attr("class", ..)` replaces; every element, void ones included, is rendered with a closing tag (`<input></input>`); attributes come out in alphabetical order.

## 2. Raw text: inline scripts and styles

`style(..)` and `script(..)` take a `String` and put it in the element as text. Call `.raw()` or the content is HTML-escaped (a `>` in CSS or `&&` in JS would break).

```rust
let tag = script("if (a && b) { go(); }".to_string()).raw().defer();
assert_eq!(tag.render(), "<script defer>if (a && b) { go(); }</script>");
```

Pitfall: `.raw()` also turns off escaping for user-supplied text. Never use it on data you did not write.

## 3. A whole document

Goal: a page that is not part of the shell.

```rust
let html = PageBuilder::new()
    .title("Report")
    .links(vec![Link::new("stylesheet", "/report.css")])
    .scripts(vec![Script::new("/report.js")]) // external: defer by default
    .content(div().child(h1().text("Report")))
    .build();
```

The output is pretty-printed, four spaces per level, and starts with `<!DOCTYPE html>` (the pretty printer drops declarations, so `build` adds the line back; see [Internals](internals.md#the-pretty-printer)). Void elements are written with a closing tag (`<meta ...>` then `</meta>`). `build()` panics when `.content(..)` was not called.

## 4. The app shell for a service

Goal: a page with header, settings panel, footer, themes and locales.

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

What `build()` does on disk and with the working directory is in [Configuration](configuration.md). You must create `i18n/<locale>.ftl` files containing at least `header_label`, `footer_label`, `locale_label` and `theme_label` (the fixtures in `tests/fixtures/baseline/i18n/` are a template):

```text
# required
header_label = Quench - sample web app
footer_label = Footer text
locale_label = Language
theme_label = Theme
```

Serve `dist/assets` at `{resources_prefix}/assets/`. Pitfalls: `build()` panics when a locale passed to `.supported_locales(..)` has no `.ftl` file (use `try_build()` to get an `Err`); the working directory must be the directory that contains `i18n/`.

## 5. A page without chrome (login)

```rust
let shell = AppShellBuilder::new().with_header(false).build();
let login_page = shell.page(div().class("login-card").child(h1().text("Sign in")));
```

`with_header(false)` omits the header; the footer is still rendered. To drop the footer too, build the page with `AppBuilder` directly and do not call `.footer(..)`.

## 6. Navigation without the panel

```rust
let shell = AppShellBuilder::new().with_nav(false).build();
```

The header keeps its title but has neither the grip button nor the settings panel, so users cannot switch theme or language from the page (the cookies are still honoured).

## 7. Custom header, extra assets, own scripts

```rust
let shell = AppShellBuilder::new()
    .header(header().class("q-shell-header").child(h2().text("My service")))
    .links(vec![Link::new("stylesheet", "/static/app.css")])
    .scripts(vec![js!("window.API_BASE = '{}';", "/api/v1").defer()])
    .build();
```

The custom scripts come after the shell's own, so they can use `getLocale()`, `qT(key)` and `updateTheme(name)`. Pitfall: a custom header replaces the generated one entirely, including the navigation panel.

## 8. Drive translated text from code

Elements are translated in the browser, not on the server: put the catalog key in `data-i18n`.

```rust
let greeting = p().attr("data-i18n", "greeting");
let with_args = p()
    .attr("data-i18n", "welcome")
    .attr("data-i18n-args", r#"{"name":"Ada"}"#); // replaces {$name} in the message
```

The server-rendered text (none here) is shown until the script runs; elements whose key is missing in the active catalog are left unchanged.

## 9. Generate CSS rules

```rust
let css = CssRule::new(".badge")
    .property("padding", "0.2rem 0.5rem")
    .child(CssRule::new("&.ok").property("color", "green"))
    .render();
```

The output uses nested CSS (`&`), which modern browsers support.
