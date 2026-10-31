# quench-layout
Box layout with no dependencies: block and inline flow, flexbox, positioning, margin collapsing, over computed styles from `quench-css` and text from `quench-text`.

## What it is
`quench-layout` places boxes. It takes a tree of styled nodes (computed styles from `quench-css`, text measured with `quench-text`) and a viewport size, and returns a tree of boxes with positions and sizes: block and inline flow, flexbox, grid, positioning and margin collapsing.

`quench-paint` draws that tree; `quench-ui-native` drives the whole pipeline. Layout is a pure function of the tree, with no incremental relayout, and its types are `!Send` because nodes are shared with `Rc`.

## Use cases
- Compute box geometry for a styled tree before painting it.
- Find a box's absolute position by its key.
- Read text line fragments for carets and selections.
- Lay out replaced boxes such as images with an intrinsic size.
- Get scroll extents of overflowing containers.

## Example
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
assert!(!boxes.fragments.is_empty());      // the text is on a line
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-layout/
