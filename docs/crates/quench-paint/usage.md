---
title: Usage
parent: quench-paint
nav_order: 1
---

## 1. Style, lay out, paint

```rust
use std::rc::Rc;
use quench_css::{Cascade, ComputedStyle, Display, Element, MediaEnv, State, parse};
use quench_layout::{Fonts, Node, layout};
use quench_paint::paint;
use quench_raster::{Canvas, Color};

struct One;   // an element with no relatives, so the cascade can compute `e { ... }`
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
let card_style = style("display: block; margin: 8px; padding: 6px; background: #3b82f6; border-radius: 6px", &root_style);
let text_style = Rc::new(card_style.inherited());
let card = Node::element(2, card_style, vec![Node::text(3, &text_style, "Hello, native world")]);
let root = Node::element(1, root_style, vec![card]);

let fonts = Fonts::embedded();
let boxes = layout(&root, (200.0, 100.0), &fonts);
let mut canvas = Canvas::new(200, 100);
canvas.clear(Color::WHITE);
paint(&mut canvas, &boxes, &fonts, &|_| (0.0, 0.0), &|_| Vec::new());
assert_eq!(canvas.pixel(10, 10), Color::rgb(59, 130, 246));
```

Verified by running it. Use the same `Fonts` for layout and paint.

## 2. Scrolling

The `scroll` callback maps a box key to its scroll offset `(x, y)` in pixels. It is consulted for boxes that clip (`overflow` other than `visible` on either axis) and for the root box (key `0`, the document). A scroll container with `overflow-y: auto|scroll` and taller content gets a thin bar (6 px wide, `rgba(160,160,160,150)`) at its right edge.

```rust
# use quench_layout::*; use quench_paint::*; use quench_raster::*;
# fn f(canvas: &mut Canvas, boxes: &LayoutBox, fonts: &Fonts) {
let offsets = |key: usize| if key == 7 { (0.0, 120.0) } else { (0.0, 0.0) };
paint(canvas, boxes, fonts, &offsets, &|_| Vec::new());
# }
```

## 3. Decorations

Things the host draws on a box that are not CSS: a checkbox tick, a radio dot, a caret, an image, a `background-image`. The callback returns them by box key.

```rust
# use std::rc::Rc;
use quench_paint::Decoration;
let tick = |key: usize| if key == 4 {
    vec![Decoration::Check(quench_css::Color::BLACK)]
} else { Vec::new() };
```

`Image` and `Background` take straight-alpha RGBA bytes (`width * height * 4`) in an `Rc<Vec<u8>>`.

## 4. Hit-testing order

`Layers::of(&box)` gives the children grouped in paint order (`below`, `blocks`, `inline`, `above`); a pointer search walks them in reverse.
