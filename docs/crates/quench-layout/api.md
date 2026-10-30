---
title: API reference
parent: quench-layout
nav_order: 2
---

Crate root re-exports (`src/lib.rs`):

```rust
pub use fonts::{Fonts, Metrics};
pub use inline::LINE_BREAK;
pub use node::{Content, Fragment, InlineFragment, LayoutBox, Node, TextFragment};
pub use position::layout;
```

Modules are private. 9 types (`Fonts`, `Metrics`, `Node`, `Content`, `Fragment`, `TextFragment`, `InlineFragment`, `LayoutBox`, plus fields), 1 function (`layout`), 1 constant (`LINE_BREAK`). No `unsafe`, no features. `Node` is shared through `Rc`, so the tree is `!Send`. (`Pending` is `pub` in the private module `node` but not reachable.)

## `layout`

```rust
#[must_use]
pub fn layout(root: &Rc<Node>, viewport: (f32, f32), fonts: &Fonts) -> LayoutBox
```

Lays out `root` in a viewport of `(width, height)` pixels. The root box is at `(margin-left, margin-top)`; its width fills the viewport (minus margins) unless its style says otherwise; the viewport height is the `avail_h` for percentage heights. Absolutely positioned boxes resolve against the nearest positioned ancestor's padding box, or the viewport; `fixed` boxes against the viewport.

## `Node`

```rust
#[derive(Debug)]
pub struct Node { pub key: usize, pub style: Rc<ComputedStyle>, pub content: Content }
```

`key` is the caller's name for the node (for example an index into its DOM) and comes back on the boxes. The key `usize::MAX` is used by the engine for the anonymous box it wraps around text that is a direct child of a flex or grid container.

| Constructor | Signature |
|---|---|
| `Node::element` | `#[must_use] pub fn element(key: usize, style: ComputedStyle, children: Vec<Rc<Self>>) -> Rc<Self>` |
| `Node::text` | `#[must_use] pub fn text(key: usize, style: &Rc<ComputedStyle>, text: &str) -> Rc<Self>`: the style is that of the element the characters are in; text is split into words by Unicode whitespace |
| `Node::replaced` | `#[must_use] pub fn replaced(key: usize, style: ComputedStyle, width: f32, height: f32) -> Rc<Self>` |

## `Content`

```rust
#[derive(Debug)]
pub enum Content {
    Text(String),
    Replaced(f32, f32),               // (width, height) in pixels: a checkbox, an image
    Children(Vec<Rc<Node>>),
}
```

## `Fragment`, `TextFragment`, `InlineFragment`

```rust
#[derive(Debug, Clone)]
pub enum Fragment { Text(TextFragment), Inline(InlineFragment) }

#[derive(Debug, Clone)]
pub struct TextFragment { pub key: usize, pub text: String, pub x: f32, pub baseline: f32, pub width: f32, pub style: Rc<ComputedStyle> }

#[derive(Debug, Clone)]
pub struct InlineFragment { pub key: usize, pub style: Rc<ComputedStyle>, pub x: f32, pub y: f32, pub width: f32, pub height: f32, pub first: bool, pub last: bool }
```

| Field | Meaning |
|---|---|
| `TextFragment::key` | key of the text node |
| `TextFragment::text` | characters, after `text-transform`; words joined by single spaces; a trailing space at the end of a line is removed |
| `TextFragment::x`, `baseline`, `width` | from the top left of the **content box of the box that holds the lines** (shifted by its border and padding, so effectively its border box origin); `baseline` is a y coordinate |
| `InlineFragment::first` / `last` | the element starts / ends on this fragment (borders and radii only at those ends) |
| `InlineFragment::y`, `height` | top of the inline box (baseline - ascent - padding-top - border-top) and `ascent + descent + vertical padding + vertical border` of the element's own font |

## `LayoutBox`

```rust
#[derive(Debug, Clone)]
pub struct LayoutBox {
    pub key: usize,
    pub style: Rc<ComputedStyle>,
    pub x: f32, pub y: f32, pub width: f32, pub height: f32,
    pub margin: [f32; 4], pub padding: [f32; 4], pub border: [f32; 4],  // top, right, bottom, left
    pub children: Vec<Self>,
    pub fragments: Vec<Fragment>,
    pub baseline: f32,
    pub overflow: (f32, f32),
    /* pub(crate) pending */
}
```

`x`, `y`, `width`, `height` are those of the **border box**; `x`, `y` are from the border box of the parent. `children` holds blocks, flex/grid items, inline blocks (atoms), and positioned boxes, in that construction order (positioned boxes are appended last, not in document order). `baseline` is the first baseline from the top of the border box (the height if there is none). `overflow` is how far the contents reach.

| Method | Signature | Behaviour |
|---|---|---|
| `content_rect` | `#[must_use] pub fn content_rect(&self) -> (f32, f32, f32, f32)` | `(x, y, w, h)` of the content box from the border box top left |
| `padding_rect` | `#[must_use] pub fn padding_rect(&self) -> (f32, f32, f32, f32)` | same for the padding box |
| `text_fragments` | `#[must_use] pub fn text_fragments(&self, key: usize) -> Vec<(f32, f32, &TextFragment)>` | all fragments of text node `key` as `(x, baseline, fragment)` from the root's border box; `transform: translate` of boxes is included |
| `find` | `#[must_use] pub fn find(&self, key: usize) -> Option<&Self>` | depth-first search by key |
| `position_of` | `#[must_use] pub fn position_of(&self, key: usize) -> Option<(f32, f32)>` | border box position from the root's border box |

## `Fonts` and `Metrics`

```rust
pub struct Fonts { pub light: FontSet, pub regular: FontSet, pub medium: FontSet, pub bold: FontSet, /* private cache */ }
// Debug prints "Fonts { .. }"

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics { pub ascent: f32, pub descent: f32, pub normal: f32 }
```

| Method | Signature | Behaviour |
|---|---|---|
| `Fonts::embedded` | `#[must_use] pub fn embedded() -> Self` | the four embedded Noto Sans weights, no fallback |
| `Fonts::system` | `#[must_use] pub fn system() -> Self` | the same with `with_system_fonts()` on each set |
| `Fonts::set` | `#[must_use] pub const fn set(&self, style: &ComputedStyle) -> &FontSet` | by `font_weight`: 0-350 light, 351-449 regular, 450-599 medium, 600+ bold. Family and italic do not select a face. |
| `Fonts::metrics` | `#[must_use] pub fn metrics(&self, style: &ComputedStyle) -> Metrics` | `ascent` and `descent` rounded to whole pixels; `normal = ascent + descent`. For Noto Sans at 16 px the normal line is 22 px. |
| `Fonts::measure` | `#[must_use] pub fn measure(&self, style: &ComputedStyle, text: &str) -> f32` | width of one line of text at `style.font_size` |
| `Fonts::space` | `#[must_use] pub fn space(&self, style: &ComputedStyle) -> f32` | width of a space, measured as `"x x" - "xx"`; cached per (font size bits, face) |
| `Fonts::line_height` | `#[must_use] pub fn line_height(&self, style: &ComputedStyle) -> f32` | `normal` -> `metrics.normal`; `Number(n)` -> `font_size * n`; `Px(px)` -> `px` |

## `LINE_BREAK`

```rust
pub const LINE_BREAK: char = '\u{2028}';
```

U+2028 in a text node ends the line (the host puts it for `<br>`).
