---
title: quench-ui
parent: Crates
nav_order: 56
has_children: true
permalink: /crates/quench-ui/
---

`quench-ui` (library `quench_ui`) is the platform-independent core of the Quench UI: fine-grained reactivity (signals, memos, effects, scopes), a view tree, typed events and the `Renderer` trait that a backend implements. It replaces a UI framework such as Leptos or Yew. It has no dependencies and no `unsafe` code (`#![forbid(unsafe_code)]`), and it builds for `wasm32-unknown-unknown` and for native hosts.

Nothing in the core knows about the DOM. Backends plug in behind `Renderer`: the browser backend (`quench-ui-web`), the self-drawing native backend (`quench-ui-native`), the in-memory `TestRenderer` in this crate, and `render_to_string`, which uses the in-memory tree to produce HTML for server-side rendering.

Everything runs on one thread (`Rc`/`RefCell`). Signals, memos, effects, scopes, views and mounts are `!Send` and `!Sync`.

Version: **0.1.3**. Edition: 2024 (workspace). No `rust-version` is set.

## Install

```toml
[dependencies]
quench-ui = { version = "0.1.3", registry = "ennor" }
```

Inside the workspace the dependency is `quench-ui = { workspace = true }`. The workspace table pins `version = "0.1.0"` with a `path`, which is satisfied by the 0.1.3 path crate.

## Features

None. `Cargo.toml` has no `[features]` table.

## Dependencies

| Dependency | Kind | Why |
|---|---|---|
| none | normal | The core uses only `std`. |
| `quench-test` | dev | The unit tests use `quench_test::main!()` and `#[quench_test::test]` (the crate has `harness = false`). |

## Modules

| Module | Page | Content |
|---|---|---|
| `reactive` | [api-reactive](api-reactive.md) | Signals, memos, effects, scopes, context. |
| `view` | [api-view](api-view.md) | `View`, `Element`, `IntoView`, `Key`, tag helpers. |
| `event`, `renderer`, `html`, `test_renderer`, `virtual_list`, `mount` | [api-render](api-render.md) | Events, the `Renderer` trait, mounting, escaping, the test renderer, virtual lists. |
| `router` | [api-router](api-router.md) | Routing and history. |
| `form` | [api-form](api-form.md) | Fields, rules, forms. |
| `i18n` | [api-i18n](api-i18n.md) | Translations. |
| `kit` | [api-kit](api-kit.md) | Component kit. |
| `platform`, `anim`, `gesture` | [api-platform](api-platform.md) | Host services, animation, gestures. |

## Example

A counter, built into the in-memory renderer (this is the crate-level doctest):

```rust
use std::rc::Rc;
use quench_ui::prelude::*;

let renderer = Rc::new(TestRenderer::new());
let root = renderer.root();

let count = create_signal(0);
let (shown, clicked) = (count.clone(), count.clone());
let view = div()
    .child(span().text_fn(move || format!("count: {}", shown.get())))
    .child(button().text("+").on_click(move || clicked.update(|c| *c += 1)));

let mounted = mount(&renderer, &root, view);
assert_eq!(renderer.html(), "<div><span>count: 0</span><button>+</button></div>");

let button = renderer.find_by_tag("button")[0];
renderer.fire(button, &Event::Click);
assert_eq!(renderer.text(), "count: 1+");
mounted.unmount();
```

## Where to go next

* [Usage guide](usage.md): signals, effects, memos, scopes, views, keyed lists, events, testing, server rendering, context.
* Reference: [reactive](api-reactive.md), [view](api-view.md), [events, renderer, mount, HTML, test renderer, virtual list](api-render.md).
* [Internals](internals.md): the dependency graph, the 100,000-run limit, escaping, keyed reconciliation.
* [Testing](testing.md): how the crate is tested and how to test components.
* Application-level usage (routing, forms, translations): [usage-app](usage-app.md).
* Reference for the other modules: [router](api-router.md), [form](api-form.md), [i18n](api-i18n.md), [kit](api-kit.md), [platform, anim, gesture](api-platform.md).
