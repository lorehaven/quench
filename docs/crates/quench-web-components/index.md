---
title: quench-web-components
parent: Crates
nav_order: 61
has_children: true
permalink: /crates/quench-web-components/
---

`quench-web-components` is a small set of reusable widget builders on top of `quench-web`'s `Element`: buttons, cards, panels, containers, form fields, status badges and alert boxes, plus the shell components re-exported from `quench-web`. Each builder only produces an `Element` with class names; it carries no styling of its own.

| | |
|---|---|
| Crate / library name | `quench-web-components` / `quench_web_components` |
| Version | 0.1.20 |
| Edition | 2024 (workspace) |
| Unsafe code | none |

## Install

```toml
[dependencies]
quench-web-components = { version = "0.1", registry = "ennor" }
quench-web = { version = "0.1", registry = "ennor" }
```

`Element` and the tag helpers come from `quench-web`; add it as a dependency too (this crate does not re-export `Element`).

## Feature flags

None.

## Dependencies

| Dependency | Why |
|---|---|
| `quench-web` | `Element`, tag helpers, and the `components` / `dom` modules that are re-exported here. |
| `quench-test` (dev) | Test harness. |

## Where to go next

* [Usage](usage.md)
* [API: widgets](api-widgets.md): buttons, containers, forms, status.
* [API: re-exports](api-components.md): `components`, `dom`, `prelude`.
* [CSS classes](internals.md#css-classes-emitted): what each builder writes and which classes the shared stylesheet actually defines.
* [Testing](testing.md)

## Minimal example

```rust
use quench_web_components::containers::Card;
use quench_web_components::status::success_badge;

fn widget() -> quench_web::Element {
    Card::new()
        .title("Deployment")
        .content(success_badge("healthy"))
        .build()
}
```

Renders (via `.render()`):

```html
<div class="card"><div class="card-header"><h3>Deployment</h3></div><div class="card-body"><span class="status-badge status-success">healthy<i class="fas fa-check-circle"></i></span></div></div>
```
