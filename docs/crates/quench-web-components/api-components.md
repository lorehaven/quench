---
title: Re-exports
parent: quench-web-components
grand_parent: Crates
nav_order: 2
---

Three modules of this crate are thin re-exports of `quench-web`, kept so old `quench_web_components::...` paths keep compiling. Their items are documented in [quench-web: Components and DOM helpers](../quench-web/api-components.md).

## `components`

```rust
#[doc(inline)]
pub use quench_web::framework::components::*;
#[doc(inline)]
pub use quench_web::framework::components::{footer, header, nav};
```

Items: `HeaderBuilder`, `FooterBuilder`, `NavPanelBuilder`, `nav_button`, `locale_switch`, and the submodules `footer`, `header`, `nav`. Also glob re-exported at the crate root.

## `dom`

```rust
#[doc(inline)]
pub use quench_web::framework::dom::*;
#[doc(inline)]
pub use quench_web::framework::dom::{events, modal, select};
```

Items: `on_dom_ready`, `toggle_modal`, `update_from_select`, `set_select_value`, and the submodules `events`, `modal`, `select`.

## `prelude`

```rust
pub use crate::components::{FooterBuilder, HeaderBuilder, NavPanelBuilder};
pub use crate::dom::{on_dom_ready, set_select_value, toggle_modal, update_from_select};
pub use crate::components::nav_button;
```

The prelude contains only the shell components and DOM helpers. **It does not include the widgets** (`ButtonBuilder`, `Card`, `FormInput`, `StatusBadge`, ...) nor `locale_switch`; import those from the crate root or their module.

## Crate root

`lib.rs` declares `pub mod buttons; components; containers; dom; forms; prelude; status;` and glob re-exports all of them except `prelude`. Name clashes are avoided because module names differ from item names, with one exception to know: `quench_web_components::footer` and `quench_web_components::header` (submodules of `components`) are modules, not the `quench_web::footer()` / `header()` tag helpers.
