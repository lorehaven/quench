---
title: quench-layout
parent: Crates
nav_order: 28
has_children: true
---

`quench-layout` (crate `quench_layout`) places boxes. It takes a tree of styled nodes (computed styles from [quench-css](../quench-css/index.md), text measured with [quench-text](../quench-text/index.md)) and a viewport size, and returns a tree of boxes with positions and sizes: block and inline flow, flexbox, grid, positioning, margin collapsing. [quench-paint](../quench-paint/index.md) draws that tree; [quench-ui-native](../quench-ui-native/index.md) drives the whole pipeline.

* Version: 0.1.5. Edition 2024. No features. No `rust-version`.
* Dependencies: `quench-css` (the `ComputedStyle` it reads), `quench-text` (fonts, measuring). Dev: `quench-test`.
* All types are `!Send` (nodes are shared with `Rc`).

```toml
[dependencies]
quench-layout = "0.1"
quench-css = "0.1"
```

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Supported layout](supported.md): which display modes and properties are honoured, per property, and the limits.
* [Internals](internals.md): the algorithms (block, inline, flex, grid, positioning, intrinsic sizes).
* [Testing](testing.md)

## Minimal example

```rust
use std::rc::Rc;
use quench_css::{ComputedStyle, Display};
use quench_layout::{Fonts, Node, layout};

let mut root_style = ComputedStyle::default();
root_style.display = Display::Block;
let text_style = Rc::new(root_style.clone());
let root = Node::element(1, root_style, vec![Node::text(2, &text_style, "Hello")]);

let boxes = layout(&root, (390.0, 844.0), &Fonts::embedded());
assert_eq!(boxes.width, 390.0);           // a block fills the viewport
assert_eq!(boxes.key, 1);
assert!(!boxes.fragments.is_empty());      // the text is on a line
```

Checked against the crate's tests, which build nodes through the real cascade (see [Usage](usage.md)).
