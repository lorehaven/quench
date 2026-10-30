---
title: DOM API
parent: quench-ui-native
nav_order: 4
---

Source: `src/dom.rs` and `src/lib.rs`. The public surface of the crate is exactly what `lib.rs` exports:

```rust
pub use access::{AccessAction, AccessNode, Role};
pub use dom::{NativeRenderer, NodeId, NodeKind};
pub use input::ImeState;
pub use pipeline::UA_STYLESHEET;
pub use quench_css::{ColorScheme, MediaEnv, Pointer};
```

`dom.rs` also declares `pub type ImageLoader`, `pub struct Picture` and `From<quench_image::Image> for Picture`; they sit in a private module and are not re-exported, so they are not reachable from outside the crate.

## Re-exports

* `ColorScheme`: `enum { Light, Dark }` (`Debug, Clone, Copy, PartialEq, Eq` as defined in quench-css).
* `Pointer`: `enum { None, Coarse, Fine }`, the main pointing device.
* `MediaEnv`: `struct { pub width: f32, pub height: f32, pub color_scheme: ColorScheme, pub reduced_motion: bool, pub hover: bool, pub pointer: Pointer }`, `Debug, Clone, Copy, PartialEq`. `Default` is 1280 x 800, `ColorScheme::Dark`, `reduced_motion: false`, `hover: true`, `Pointer::Fine`.

These belong to [quench-css](../quench-css/index.md); only the use here is described.

## NodeId

```rust
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(/* private usize */);

impl NodeId {
    #[must_use]
    pub const fn index(self) -> usize;
}
```

A handle to a node of one renderer: the index into the node arena. `index()` is the number the layout gives the node's box as its `key` and the `id` of the accessibility node. `NodeId`s are only meaningful for the renderer that created them. The `<html>` element is index 0 and `<body>` index 1; nodes created later get increasing numbers. The field is private, so a `NodeId` cannot be built from a number; use the ids that come back from the renderer (`root`, `children`, `focused`, `element_at`). Accessibility actions take the raw `usize` (`perform(id: usize, ...)`).

## NodeKind

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Element(String), // the lower-case tag
    Text(String),
    Anchor,
}
```

What a node is, for code that walks the document. `Anchor` is the empty marker node `quench-ui` uses for dynamic regions (`create_anchor`); it produces no box and no text.

## NativeRenderer

```rust
pub struct NativeRenderer { /* private */ }
impl std::fmt::Debug for NativeRenderer { /* prints `NativeRenderer { nodes: N, .. }` */ }
impl Default for NativeRenderer { /* Self::new() */ }
```

`!Send`, `!Sync`. All methods take `&self`; state is behind `RefCell`s, so re-entrant calls from inside a listener are safe except where a method panics on a double borrow (the renderer releases its borrows before it calls listeners; listeners may call back into the renderer).

### new

```rust
#[must_use]
pub fn new() -> Self
```

A document `<html><body></body></html>` (html = node 0, body = node 1). Defaults: viewport 1280 x 800, scale 1.0, safe area `[0.0; 4]`, `MediaEnv::default()`, no stylesheets, no image loader, caret visible, dirty flag set. Loads system fonts (`Fonts::system()`). **Installs a platform** for `quench-ui` on the current thread (`quench_ui::platform::install`): a monotonic clock, one-shot timers, and the reduced-motion preference (see [timers](api-timers.md)). Installing replaces any earlier platform of that thread.

### root

```rust
#[must_use]
pub const fn root(&self) -> NodeId
```

The `<body>` (`NodeId` with index 1): where views are mounted.

### add_stylesheet

```rust
pub fn add_stylesheet(&self, css: &str)
```

Appends a stylesheet. Order: the browser's own sheet ([`UA_STYLESHEET`](api-pipeline.md#ua_stylesheet)) first, then sheets in the order added. Marks the document dirty and stale and drops the cached cascade. The text is parsed at the next layout, not here; a parse problem does not surface as an error here.

### set_viewport

```rust
pub fn set_viewport(&self, width: f32, height: f32)
```

The window size in device pixels. `media.width`/`media.height` become `width / scale`, `height / scale`. Restyles.

### set_scale

```rust
pub fn set_scale(&self, scale: f32)
```

Device pixels per CSS pixel. The value is clamped to at least 0.1. Lengths in the page are multiplied by it (absolute units, the root font size 16 px x scale, `thin`/`medium` borders), `@media` sees the window in CSS pixels, and `env()` stays right. Cached SVG pictures are dropped so they are drawn again at the new density. Pointer positions, the viewport and the safe area stay in device pixels.

### set_media

```rust
pub fn set_media(&self, media: MediaEnv)
```

Sets the colour scheme, `hover`, `pointer` and `reduced_motion`; the `width` and `height` fields of the argument are ignored and replaced by `viewport / scale`. `reduced_motion` is also pushed to the platform, so `quench_ui::platform::prefers_reduced_motion()` follows it, transitions are skipped and animations show the plain style.

### set_safe_area

```rust
pub fn set_safe_area(&self, insets: [f32; 4])
```

What `env(safe-area-inset-top|right|bottom|left)` is: `[top, right, bottom, left]`.

### set_image_loader

```rust
pub fn set_image_loader(&self, loader: impl Fn(&str) -> Option<Vec<u8>> + 'static)
```

How a `src` that is not a `data:` URL is fetched (an `<img src>` or a `background-image: url()`). Returning `None` means "not available". Previously failed images (cached `None`) are forgotten so they are tried again; decoded pictures stay.

### add_image

```rust
pub fn add_image(&self, src: &str, bytes: &[u8]) -> Result<(), quench_image::Error>
```

Registers the bytes of the image a `src` names so no loader is needed. The bytes are decoded now (raster formats of `quench-image`, else an SVG text is drawn at the current scale). **Errors:** `quench_image::Error` when the bytes are neither a decodable image nor readable SVG (`Corrupt("not a readable drawing")` for the SVG fallback) or the decoder reports an error other than "unsupported/unknown". On success it replaces any picture stored under that `src`.

### tag

```rust
#[must_use]
pub fn tag(&self, node: NodeId) -> Option<String>
```

The lower-case tag of an element; `None` for text and anchors. (Tags are lower-cased in `create_element`.)

### attr

```rust
#[must_use]
pub fn attr(&self, node: NodeId, name: &str) -> Option<String>
```

The value of an attribute (exact-name match, case-sensitive); `None` for a missing attribute or a non-element.

### children

```rust
#[must_use]
pub fn children(&self, node: NodeId) -> Vec<NodeId>
```

Child nodes in document order (elements, text, anchors). Panics if `node` is not a node of this renderer (index out of range).

### kind

```rust
#[must_use]
pub fn kind(&self, node: NodeId) -> NodeKind
```

The [`NodeKind`](#nodekind). Panics on an index out of range.

### attrs

```rust
#[must_use]
pub fn attrs(&self, node: NodeId) -> Vec<(String, String)>
```

Attributes in the order they were first set (re-setting keeps the slot; removing and setting again appends). Empty for non-elements. Includes attributes the renderer itself sets: `checked` (toggled by clicks), `value` (text fields, sliders).

### styles

```rust
#[must_use]
pub fn styles(&self, node: NodeId) -> Vec<(String, String)>
```

The properties set through `set_style` (the "style object"), not the `style` attribute. Empty for non-elements.

### text_of

```rust
#[must_use]
pub fn text_of(&self, node: NodeId) -> String
```

All text under `node` concatenated in document order (recursing through elements; anchors contribute nothing).

## The Renderer implementation

`impl quench_ui::Renderer for NativeRenderer { type Node = NodeId; ... }`. See [quench-ui render API](../quench-ui/api-render.md) for the trait. All mutators mark the document dirty and stale (layout and style are redone at the next `render`/`layout`), except `listen` and `trap_focus`, which change nothing visible.

| Method | Behaviour |
|---|---|
| `create_element(tag)` | New element with `tag.to_ascii_lowercase()`, no attributes, handlers or styles; not attached. |
| `create_text(text)` | New text node. |
| `create_anchor()` | New `Anchor` node. |
| `set_text(node, text)` | Replaces the text of a text node; no effect on other kinds. |
| `set_attr(node, name, value)` | Sets or replaces an attribute (keeps position when replacing). No effect on non-elements. The `style` attribute is parsed as declarations at style time. |
| `remove_attr(node, name)` | Removes every attribute with that name. |
| `insert(parent, child, before)` | Detaches `child` from its current parent, then inserts it before `before` or at the end. **Panics** with ``"`before` is not a child of `parent`"`` when `before` is given and is not a child of `parent`. |
| `remove(parent, child)` | Detaches `child` only if `parent` is its parent; the node stays in the arena and keeps its own children. |
| `listen(node, kind, handler)` | Adds a handler for `EventKind` to an element (several allowed; run in order of registration). Ignored on non-elements. |
| `focus(node)` | Moves focus to `node` as a mouse focus (`:focus-visible` only on text fields); fires `Blur` on the previous node and `Focus` on this one; puts the caret at the end of a field. No effect if it already has focus. Does not check focusability. |
| `trap_focus(node)` | Sets the Tab scope: Tab and Shift+Tab cycle only through focusable elements under `node`. There is no way to release the trap through the trait; set it to another node. |
| `set_style(node, property, value)` | Removes any earlier entry for `property`, then adds `(property, value)` unless `value` is empty. At style time these are appended to the `style` attribute as `;property:value`, so they win over rules and over the attribute. |

Not overridden: `prevent_default(node, kind, how)` keeps the trait's no-op default. The renderer's way of preventing defaults is the thread-local request set by `quench_ui::event::prevent_default()` during a listener (see [input](api-input.md#events-and-default-actions)).
