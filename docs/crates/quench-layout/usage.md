---
title: Usage
parent: quench-layout
nav_order: 1
---

## 1. Lay out a styled tree

A `Node` has a caller-chosen `key` (comes back on the box), a shared `Rc<ComputedStyle>` and content: children, text or a replaced box with its own size. Computed styles come from the `quench-css` cascade. The following is the helper pattern used by the crate's tests: a fake element so the cascade can compute `e { ... }`.

```rust
use std::rc::Rc;
use quench_css::{Cascade, ComputedStyle, Display, Element, MediaEnv, State, parse};
use quench_layout::{Fonts, Node, layout};

struct One;   // an element with no relatives
impl Element for One {
    fn tag(&self) -> String { "e".into() }
    fn id(&self) -> Option<String> { None }
    fn has_class(&self, _: &str) -> bool { false }
    fn attribute(&self, _: &str) -> Option<String> { None }
    fn parent(&self) -> Option<Self> { None }
    fn previous_sibling(&self) -> Option<Self> { None }
    fn state(&self, _: State) -> bool { false }
}

fn style(decls: &str, parent: &ComputedStyle) -> ComputedStyle {
    let mut cascade = Cascade::new(MediaEnv::default());
    cascade.add(&parse(&format!("e {{ {decls} }}")));
    cascade.compute(&One, Some(parent), 16.0)
}

let mut root_style = ComputedStyle::default();
root_style.display = Display::Block;
let card_style = style("display: block; margin: 8px; padding: 6px; background: #3b82f6", &root_style);
let text_style = Rc::new(card_style.inherited());
let card = Node::element(2, card_style, vec![Node::text(3, &text_style, "Hello, native world")]);
let root = Node::element(1, root_style, vec![card]);

let boxes = layout(&root, (200.0, 100.0), &Fonts::embedded());
assert_eq!(boxes.position_of(2), Some((8.0, 8.0)));
let card = boxes.find(2).unwrap();
assert_eq!((card.width, card.height), (184.0, 34.0));   // 6 + 22 (Noto Sans 16px line) + 6
```

(Verified by running it.) `x`/`y` of a box are those of its border box relative to its **parent's** border box; `position_of(key)` accumulates to the root.

## 2. Text lines

Text and inline elements of a block become `fragments` of that block: `Fragment::Text` (a run of text with `x`, `baseline`, `width`) and `Fragment::Inline` (the extent of an inline element on one line, for its background and border).

```rust
# use quench_layout::{LayoutBox, Fragment};
fn lines(b: &LayoutBox) -> Vec<(String, f32, f32)> {
    b.fragments.iter().filter_map(|f| match f {
        Fragment::Text(t) => Some((t.text.clone(), t.x, t.baseline)),
        Fragment::Inline(_) => None,
    }).collect()
}
```

`LayoutBox::text_fragments(key)` returns every fragment of a text node with absolute positions (a caret or selection helper).

## 3. A `<br>`

Put `quench_layout::LINE_BREAK` (U+2028) in a text node. The tokenizer ends the line there.

## 4. Replaced boxes (images, checkboxes)

`Node::replaced(key, style, width, height)` is a box with an intrinsic size. With only `width` (or only `height`) given in the style the other follows the proportions.

## 5. Fonts

`Fonts::embedded()` uses only the embedded Noto Sans (deterministic: what tests want). `Fonts::system()` adds the machine's `.ttf` fonts behind each face for characters Noto lacks. The same `Fonts` must be given to `quench_paint::paint`, since text is measured here and drawn there with the same faces.

## 6. Scrolling extents

`LayoutBox::overflow` is `(w, h)`, how far the box's contents reach from its border box top left (never less than the box's own size). A scroll container compares it with its padding box.

Pitfalls: a node with `display: none` is skipped entirely (no box). Text nodes need an inherited style (`ComputedStyle::inherited()`) with `display: inline`. Layout is a pure function of the tree; call it again after any change (there is no incremental relayout).
