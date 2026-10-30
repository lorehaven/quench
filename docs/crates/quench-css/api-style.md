---
title: "API: style"
parent: "quench-css"
nav_order: 8
---

Module `style`: the cascade (`Cascade`), the typed result (`ComputedStyle`) and the value types it uses. Which CSS maps to which field is on [Supported CSS](supported.md#properties).

## Value enums

All derive `Debug, Clone, Copy, PartialEq, Eq` unless noted.

| Type | Variants | Notes |
|---|---|---|
| `Display` | `None, Block, Inline, InlineBlock, Flex, InlineFlex, Grid, InlineGrid, ListItem` | |
| `Position` | `Static, Relative, Absolute, Fixed` | `sticky` parses as `Relative`. |
| `BorderStyle` | `None, Solid, Dashed, Dotted` | Also used for `outline-style`. |
| `BackgroundSize` | `Auto, Cover, Contain` | |
| `Overflow` | `Visible, Hidden, Scroll, Auto` | `clip` parses as `Hidden`. |
| `FlexDirection` | `Row, RowReverse, Column, ColumnReverse` | |
| `Align` | `Stretch, Start, End, Center, Baseline` | |
| `Justify` | `Start, End, Center, SpaceBetween, SpaceAround, SpaceEvenly` | |
| `TextAlign` | `Start, End, Left, Right, Center` | |
| `TextTransform` | `None, Uppercase, Lowercase, Capitalize` | |
| `ListStyle` | `None, Disc, Decimal` | |
| `Timing` | `Linear, Ease (default), EaseIn, EaseOut, EaseInOut` | `Default` is `Ease`. |
| `Direction` | `Normal (default), Reverse, Alternate, AlternateReverse` | Animation direction. |
| `FillMode` | `None (default), Forwards, Backwards, Both` | |
| `GridLine` | `Auto, Line(i32), Span(u32)` | `Line` is from 1; negative counts from the end. |
| `LineHeight` (`PartialEq` only) | `Normal, Number(f32), Px(f32)` | `Number` is a multiple of the font size. |

## Grid and animation structs

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Track {
    Length(Length),
    Fr(f32),
    Auto,
    MinMax(Box<Track>, Box<Track>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrackList {
    pub tracks: Vec<Track>,
    pub repeat_to_fill: Option<(Vec<Track>, bool)>,
}
```

`Track::Auto` is `auto`, `min-content` and `max-content`. `TrackList.repeat_to_fill` is `repeat(auto-fill | auto-fit, ...)`: the tracks repeated after `tracks` as many times as fit; the bool is `true` for `auto-fit` (collapse empty tracks). `Default` is empty with `None`.

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub property: String,   // lowercase property name or "all"
    pub duration: f32,      // seconds
    pub delay: f32,         // seconds
    pub timing: Timing,
}
// Default: property "all", duration 0.0, delay 0.0, Timing::Ease

#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    pub name: String,       // empty = none
    pub duration: f32,
    pub delay: f32,
    pub timing: Timing,
    pub iterations: f32,    // f32::INFINITY for `infinite`
    pub direction: Direction,
    pub fill: FillMode,
}
// Default: name "", 0.0, 0.0, Ease, 1.0, Normal, None

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub x: f32, pub y: f32, pub blur: f32, pub spread: f32,
    pub color: ColorValue,
    pub inset: bool,
}
```

## `TouchAction`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchAction { pub left: bool, pub right: bool, pub up: bool, pub down: bool }
```

The directions of *finger* movement the platform may take for itself (`up` is a finger moving up the screen, which scrolls content down).

| Item | Signature | Behaviour |
|---|---|---|
| `AUTO` | `pub const AUTO: Self` | all four `true` |
| `NONE` | `pub const NONE: Self` | all four `false` |
| `and` | `#[must_use] pub const fn and(self, other: Self) -> Self` | Field-wise AND: an element is as free as it and its ancestors allow. |
| `allows` | `#[must_use] pub fn allows(self, dx: f32, dy: f32) -> bool` | Whether the platform may take a movement `(dx, dy)`: the larger absolute axis decides (`dx` wins only if `|dx| > |dy|`), then the sign picks the direction (`dx < 0` is `left`, `dy < 0` is `up`; zero counts as `right`/`down`). |

## `Transform`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub translate: (Length, Length),
    pub scale: (f32, f32),
    pub rotate: f32,          // degrees, clockwise
}
```

`transform` and `rotate` folded into one: translations add, scales multiply, angles add. `Default`: zero translation, scale `(1.0, 1.0)`, rotation 0. `pub fn is_identity(&self) -> bool` (`#[must_use]`): true when both translations are the constant 0, scale is `(1.0, 1.0)` and rotation is 0.

## `ComputedStyle`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle { /* all fields pub */ }
```

Everything layout and painting need for one element. Sides are `[top, right, bottom, left]`; corners are `[top-left, top-right, bottom-right, bottom-left]`; `None` is `auto` (or no limit). `!Send` (it holds `Rc`). `Default` gives the initial values below; `Cascade::compute` starts a child from `parent.inherited()`.

| Field | Type | Initial (`Default`) | Inherited | From CSS |
|---|---|---|---|---|
| `animations` | `Vec<Animation>` | empty | no | `animation*` |
| `grid_areas` | `Vec<Vec<String>>` | empty | no | `grid-template-areas` (`.` for an empty cell) |
| `grid_area` | `Option<String>` | `None` | no | `grid-area: <name>` |
| `transitions` | `Vec<Transition>` | empty | no | `transition*` |
| `grid_columns`, `grid_rows` | `TrackList` | empty | no | `grid-template-columns/-rows` |
| `grid_auto_columns`, `grid_auto_rows` | `Track` | `Track::Auto` | no | `grid-auto-columns/-rows` |
| `grid_column`, `grid_row` | `(GridLine, GridLine)` | `(Auto, Auto)` | no | `grid-column`, `grid-row`, `-start/-end`, `grid-area` |
| `background_image` | `Option<String>` | `None` | no | the `url()` of `background-image` / `background` |
| `background_size` | `BackgroundSize` | `Auto` | no | |
| `background_repeat` | `bool` | `true` | no | |
| `background_position` | `(f32, f32)` | `(0.0, 0.0)` | no | fractions of free space; `(0.5, 0.5)` is `center` |
| `display` | `Display` | `Inline` | no | |
| `position` | `Position` | `Static` | no | |
| `inset` | `[Option<Length>; 4]` | all `None` | no | `top/right/bottom/left`, `inset` |
| `z_index` | `Option<i32>` | `None` | no | |
| `width`, `height` | `Option<Length>` | `None` | no | |
| `min_width`, `min_height`, `max_width`, `max_height` | `Option<Length>` | `None` | no | `None` min is `auto` |
| `border_box` | `bool` | `false` | no | `box-sizing: border-box` |
| `margin` | `[Option<Length>; 4]` | all `Some(0)` | no | `None` is `auto` |
| `padding` | `[Length; 4]` | all `0` | no | |
| `border_width` | `[f32; 4]` | `[0.0; 4]` | no | pixels (zoomed) |
| `border_style` | `[BorderStyle; 4]` | all `None` | no | |
| `border_color` | `[ColorValue; 4]` | all `CurrentColor` | no | |
| `radius` | `[Length; 4]` | all `0` | no | |
| `outline_width` | `f32` | `3.0` | no | zeroed after the cascade when `outline_style` is `None` |
| `outline_style` | `BorderStyle` | `None` | no | |
| `outline_color` | `ColorValue` | `CurrentColor` | no | |
| `outline_offset` | `f32` | `0.0` | no | |
| `background` | `ColorValue` | transparent | no | |
| `opacity` | `f32` | `1.0` | no | |
| `shadows` | `Vec<Shadow>` | empty | no | |
| `overflow` | `(Overflow, Overflow)` | `(Visible, Visible)` | no | (x, y) |
| `flex_direction` | `FlexDirection` | `Row` | no | |
| `flex_wrap` | `bool` | `false` | no | |
| `flex_grow`, `flex_shrink` | `f32` | `0.0`, `1.0` | no | |
| `flex_basis` | `Option<Length>` | `None` (auto) | no | |
| `align_items` | `Align` | `Stretch` | no | |
| `align_self` | `Option<Align>` | `None` (auto) | no | |
| `justify_content` | `Justify` | `Start` | no | |
| `row_gap`, `column_gap` | `Length` | `0` | no | |
| `color` | `Color` | `Color::BLACK` | yes | |
| `font_family` | `Rc<Vec<String>>` | empty | yes | |
| `font_size` | `f32` | `16.0` | yes | pixels (zoomed) |
| `font_weight` | `u16` | `400` | yes | |
| `italic` | `bool` | `false` | yes | `font-style` |
| `line_height` | `LineHeight` | `Normal` | yes | |
| `text_align` | `TextAlign` | `Start` | yes | |
| `text_transform` | `TextTransform` | `None` | yes | |
| `list_style` | `ListStyle` | `None` | yes | |
| `visible` | `bool` | `true` | yes | `visibility` |
| `touch_action` | `TouchAction` | `AUTO` | no | combine with `TouchAction::and` up the tree |
| `transform` | `Transform` | identity | no | `transform`, `rotate` |
| `custom` | `Rc<HashMap<String, Vec<Value>>>` | empty | yes | resolved custom properties (key includes the leading `--`) |

### Methods

```rust
#[must_use] pub fn inherited(&self) -> Self
```

The style a child starts from: `color`, `font_family`, `font_size`, `font_weight`, `italic`, `line_height`, `text_align`, `text_transform`, `list_style`, `visible` and `custom` copied; everything else `Default`.

```rust
#[must_use] pub const fn resolve(&self, color: ColorValue) -> Color
```

A colour with `currentcolor` replaced by this style's `color`.

```rust
#[must_use] pub fn line_height_px(&self, normal: f32) -> f32
```

`Normal` is `font_size * normal`, `Number(n)` is `font_size * n`, `Px(px)` is `px`. The caller supplies the font's natural ratio (the comment in the source says Noto Sans reports 1.36).

## `Cascade`

```rust
#[derive(Debug, Clone)]
pub struct Cascade {
    // private: rules, keyframes
    pub media: MediaEnv,
    pub safe_area: [f32; 4],  // env(safe-area-inset-*): top, right, bottom, left, device pixels
    pub zoom: f32,            // device pixels per CSS pixel
}
```

Style rules from one or more sheets in the order added, ready to compute styles. `!Send`.

### `Cascade::new`

```rust
#[must_use] pub fn new(media: MediaEnv) -> Self
```

Empty cascade; `safe_area = [0.0; 4]`, `zoom = 1.0`.

### `Cascade::add`

```rust
pub fn add(&mut self, sheet: &Stylesheet)
```

Appends the sheet's rules (cloned) after the existing ones and stores its keyframes by name (a later keyframes of the same name replaces the earlier). A later sheet always wins ties ([order across sheets](internals.md#order-across-sheets)).

### `Cascade::keyframes`

```rust
#[must_use] pub fn keyframes(&self, name: &str) -> Option<&Keyframes>
```

The `@keyframes` of that name; the last one added wins.

### `Cascade::style_with`

```rust
#[must_use]
pub fn style_with(&self, base: &ComputedStyle, parent: &ComputedStyle,
                  root_font_size: f32, declarations: &[Declaration]) -> ComputedStyle
```

`base` with `declarations` applied (custom-property declarations skipped; `var()`/`env()` substituted from `base.custom`; a declaration that fails substitution or parsing is skipped). Units use `base.font_size` for `em` (no font-size-first pass), `parent` for `inherit`. Does not recompute inheritance, media or the post-processing (zeroed widths, out-of-flow display).

### `Cascade::viewport`

```rust
#[must_use] pub const fn viewport(&self) -> (f32, f32)
```

`(media.width, media.height)`, unzoomed.

### `Cascade::compute`

```rust
#[must_use]
pub fn compute<E: Element>(&self, element: &E, parent: Option<&ComputedStyle>,
                           root_font_size: f32) -> ComputedStyle
```

`compute_with` with no inline declarations.

### `Cascade::compute_with`

```rust
#[must_use]
pub fn compute_with<E: Element>(&self, element: &E, parent: Option<&ComputedStyle>,
                                root_font_size: f32, inline: &[Declaration]) -> ComputedStyle
```

The algorithm, step by step, is in [Internals](internals.md#the-compute-pipeline). Summary: the style starts from `parent.inherited()`; matching rules whose `@media` levels hold contribute declarations; `inline` declarations win over all rules; declarations are sorted by (`!important`, specificity, rule order, position); custom properties are resolved first; `var()`/`env()` are substituted; `font`/`font-size` are applied first so `em` works; the rest is applied in order; `border-*-width` is zeroed where the style is `none`, likewise `outline-width`; an absolutely positioned or fixed box turns `inline`/`inline-block`/`list-item` into `block` and `inline-flex` into `flex`. With `parent == None` the parent is a default style with `font_size = 16.0 * self.zoom`. `root_font_size` is the pixel size of `rem` as given (not multiplied by `zoom`).

## `accepts`

```rust
#[must_use]
pub fn accepts(name: &str, values: &[Value]) -> bool
```

Whether the engine accepts `name: values` (against a default style, default parent and default `Units`): `false` for an unknown property, an `IGNORED` property (they have no handler), or a value that does not parse. A value with `var()` or `env()` must be substituted first. Note that some properties accept any keyword (see [Supported](supported.md#value-fallbacks)), so `accepts("display", [Ident("bogus")])` is `true`.

```rust
use quench_css::{accepts, parse_values, tokenize};

assert!(accepts("margin", &parse_values(&tokenize("1px 2px"))));
assert!(!accepts("width", &parse_values(&tokenize("red"))));
assert!(!accepts("cursor", &parse_values(&tokenize("pointer")))); // ignored, not handled
```
