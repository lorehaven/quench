---
title: Usage
parent: quench-web-components
grand_parent: Crates
nav_order: 1
---

Worked examples. Imports assume `quench-web` and `quench-web-components` are both dependencies.

## 1. Buttons

```rust
use quench_web_components::buttons::{ButtonBuilder, danger_button};

let save = ButtonBuilder::new("Save").primary().button_type("submit").id("save").build();
let remove = danger_button("Delete");
let off = ButtonBuilder::new("Wait").secondary().disabled(true).build();
```

`ButtonBuilder::new` defaults to `type="button"` so a button inside a `<form>` does not submit unless you ask. Pitfall: `.disabled(true)` only writes the attribute; nothing blocks a click handler you attached with `on_click`.

## 2. A card holding a table of values

```rust
use quench_web::prelude::*;
use quench_web_components::containers::{Card, card};

let body = ul().child(li().text("cpu: 12%")).child(li().text("mem: 3.1 GiB"));
let widget = Card::new().title("Host").content(body).footer(p().text("updated now")).build();
let quick = card("Host", p().text("ok")); // title + content only
```

Each part is optional. `title` is a plain `h3` text, **not** an i18n key: it is not translated by the shell's locale script. To translate, build the header yourself with `data-i18n`.

## 3. A form

```rust
use quench_web::prelude::*;
use quench_web_components::{buttons::primary_button, forms::*};

let f = form()
    .attr("method", "post")
    .attr("action", "/users")
    .child(FormInput::new("name").label("Name").required(true).build())
    .child(FormInput::new("password").label("Password").input_type("password").build())
    .child(
        FormSelect::new("role")
            .label("Role")
            .option("admin", "Administrator")
            .option("user", "User")
            .selected("user")
            .build(),
    )
    .child(FormTextarea::new("notes").rows(4).placeholder("Notes").build())
    .child(primary_button("Create").attr("type", "submit"));
```

The last line shows that the helper returns a plain `Element` that you can keep customising (here `attr("type", "submit")` replaces `type="button"`). Pitfalls: ids equal names, so two forms on one page with the same field name get duplicate ids; labels and placeholders are literal text.

## 4. Status badges and alerts

```rust
use quench_web_components::status::*;

let up = success_badge("running");
let custom = StatusBadge::new("pending").level(StatusLevel::Warning).build(); // no icon
let notice = AlertBox::new("Saved").success().closeable(true).build();
let err = error_alert("Could not connect");
```

`closeable(true)` renders a close icon but wires no behaviour; add a handler from your page script (the close element has class `alert-close`).

## 5. Empty lists

```rust
use quench_web_components::containers::empty_state;

let none = empty_state("no_items"); // <div class="empty" data-i18n="no_items"></div>
```

`no_items` must exist in the `i18n/*.ftl` catalogs; the shell's locale script fills the text. This is the only helper in the crate whose argument is an i18n key.

## 6. Use the shell components from this crate

```rust
use quench_web_components::prelude::*;

let header = HeaderBuilder::new().label("header_label").build();
let footer = FooterBuilder::new().label("footer_label").build();
```

These are the `quench-web` builders; see [quench-web](../quench-web/api-components.md).

## 7. Styling

The builders emit class names (`btn btn-primary`, `card`, `status-badge`, ...). The shared stylesheet that `quench-web` writes defines only a few of them (see [CSS classes](internals.md#css-classes-emitted)); services add their own rules for the rest, for example with `CssRule`.
