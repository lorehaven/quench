---
title: Pipeline API
parent: quench-ui-native
nav_order: 6
---

Source: `src/pipeline.rs`, `src/ua.css`. The pipeline turns the DOM into pixels in three stages: style, layout, paint. Two public methods drive it (`layout`, `render`) and one public constant is its first input.

## UA_STYLESHEET

```rust
pub const UA_STYLESHEET: &str = include_str!("ua.css");
```

The browser's own style sheet, parsed by the same CSS engine and added to the cascade before every page sheet (so any page rule, `style` attribute or `set_style` value wins by order and specificity as in a browser). Content of `src/ua.css` (summary; the file is the reference):

| Selector | Rules |
|---|---|
| `html` | `display: block` |
| `head, script, style, template, title, meta, link, noscript, input[type="hidden"], [hidden]` | `display: none` |
| `body` | `display: block; margin: 8px` |
| `div, section, article, aside, nav, header, footer, main, form, fieldset, address, blockquote, figure, details, summary, dl, dt, dd, p, pre, ul, ol, li, h1`-`h6`, `hr, table` | `display: block` |
| `p, blockquote, figure, dl` | `margin: 1em 0`; blockquote and figure also `margin-left/right: 40px` |
| `h1`...`h6` | font sizes 2em, 1.5em, 1.17em, (h4 default), 0.83em, 0.67em; margins 0.67em, 0.83em, 1em, 1.33em, 1.67em, 2.33em (vertical); bold |
| `b, strong, th` | bold. `i, em, cite, var, dfn`: italic. `small`: `font-size: smaller` |
| `a` | `color: #0000ee` |
| `ul, ol` | `margin: 1em 0; padding-left: 40px`; `ul { list-style: disc }`, `ol { list-style: decimal }`; `li { display: list-item }` |
| `hr` | `margin: 0.5em auto; border: 1px inset #9e9e9e; color: #9e9e9e` |
| `fieldset` | `margin: 0 2px; padding: 0.35em 0.75em 0.625em; border: 2px groove #c0c0c0` |
| `button, input, textarea, select` | `display: inline-block; font-size: 13.3333px; color: #000; text-align: start` |
| `button, select, input[type=button\|submit\|reset]` | `box-sizing: border-box` |
| `button` | `padding: 1px 6px; border: 2px outset #767676; background: #efefef; text-align: center` |
| `input, textarea` | `padding: 1px 2px; border: 2px inset #767676; background: #fff` |
| checkbox, radio | 13 x 13 px, `box-sizing: border-box`, `margin: 3px 3px 3px 4px`, 1px solid #767676 border, radius 2px (radio 50%), white; `:checked`: background and border `#0075ff`; `[disabled]`: `opacity: .5` |
| `progress, meter` | `display: inline-block; box-sizing: border-box; width: 160px; height: 16px; vertical-align: -3px; border: 1px solid #767676; border-radius: 3px; background: #e5e5e5` |
| `input[type=range]` | `box-sizing: border-box; width: 129px; height: 20px; margin: 2px; padding: 0; border: 0; background: transparent` |
| `input[type=color], input[type=file]` | empty rule |
| `img, svg, canvas, video` | `display: inline-block` |
| `label` | `display: inline` |

Pitfall: `table`/`td` get only `display: block` for `table`; this crate has no table layout.

## The three stages

### 1. Style

`NativeRenderer` builds a `quench_css::Cascade` once and caches it (dropped by `add_stylesheet`, `set_viewport`, `set_scale`, `set_media`, `set_safe_area`):

* `Cascade::new(media)`, `cascade.safe_area`, `cascade.zoom`, then `cascade.add(&parse(UA_STYLESHEET))`, then `add(&parse(sheet))` for each page sheet in order.

Per element the renderer calls `cascade.compute_with(&El { dom, id }, Some(parent_style), root_font, &inline_declarations)`:

* `El` implements `quench_css::Element` over the DOM: `tag()`; `id()` (the `id` attribute); `has_class` (whitespace-split `class`); `attribute(name)` (every attribute, so `[aria-checked]`, `[disabled]`, `[type="range"]` work); `parent()` (parent *element* only); `previous_sibling()` (previous *element* sibling, skipping text and anchors); `state(State)`:

| `State` | True when |
|---|---|
| `Disabled` | the element has a `disabled` attribute |
| `Checked` | the element has a `checked` attribute |
| `Hover` | the element is the hovered node or an ancestor of it |
| `Active` | the element is the pressed node or an ancestor of it |
| `Focus` | the element is exactly the focused node (not ancestors) |
| `FocusVisible` | focused and focus came from the keyboard (Tab, accessibility) or the focused node is a text field |

* Inline declarations: the `style` attribute text, then `;property:value` for every `set_style` entry, parsed with `parse_declarations`. They sit above all rules.
* `root_font` is `16 * scale` until the `html` element is styled, then its computed `font-size`.
* The root of the tree is node 0 (`<html>`), styled against a default `ComputedStyle` with `font_size: 16.0 * scale`.
* Text nodes get `parent.inherited()` with `display: Inline`.
* After the cascade, [`Anim::apply`](api-anim.md) puts transition and animation values into the style.
* `Anchor` nodes produce nothing.

### 2. Layout (the box tree)

The DOM is turned into a `quench_layout::Node` tree (`build`), then `quench_layout::layout(&tree, viewport, &fonts)` produces the `LayoutBox` tree. The DOM-to-node mapping does these element-specific things:

| Element | What the renderer does |
|---|---|
| `br` | One inline text child `"\u{2028}"` (line separator) with the element's inherited style; children ignored. |
| `input` of a text type | Children ignored; a single text child shows the field's text (see below); if neither `height` nor `min-height` is set, `min-height` becomes one line height (+ vertical padding and border when `box-sizing: border-box`) so an empty field is one line tall. |
| `input` of another type (`checkbox`, `radio`, `range`, `color`, `file`, `hidden`, `image`) | Children ignored, no text child. |
| `img` | Children ignored. With a decoded picture: replaced content of the image's CSS size (`width`/`height` attributes if positive numbers; only one given keeps the aspect ratio; neither: natural size = pixels / picture scale); multiplied by the page scale; `display: inline` becomes `inline-block`. Without a picture but with a non-empty `alt`: the `alt` text as an inline child. Otherwise empty. |
| `textarea` | Children ignored; shows the `value` attribute, else its own child text; if empty shows `placeholder` in grey; if neither `height` nor `min-height` is set, `min-height` = line height x `rows` (default 2). |
| `button`, and `input` of a text type | When computed `display` is `inline-block` or `block`, the box is made a flex container (`inline-flex` / `flex`) with `flex-direction: column; justify-content: center`, and the children are wrapped in one anonymous block (key `usize::MAX`), so content is centred vertically like in a browser. |
| anything else | Element children are built recursively. |

"Text types" are every `input` `type` except `checkbox`, `radio`, `range`, `color`, `file`, `hidden`, `image` (missing type = `text`).

**What an input shows** (`input_text`): the focused field shows its value with an IME composition spliced in at the selection start; an unfocused one shows the `value` attribute. `type=password` shows `U+2022` per character; `type=date` with value `Y-M-D` shows `M/D/Y`; `type=time` with value `H:M` shows `HH:M AM|PM` (12-hour; hour 0 and 12 show 12); an empty date shows `mm/dd/yyyy`, an empty time `--:-- --`; other empty types show `placeholder`. The placeholder is drawn in `rgb(117,117,117)` (not for date/time, which use the text colour).

### 3. Paint

`quench_paint::paint(canvas, &boxes, &fonts, &scroll, &decorations)` draws the boxes. `scroll(key)` returns the scroll offset of a box; `decorations(key)` returns extra drawn items per element (`Decoration` from quench-paint):

| Condition | Decoration |
|---|---|
| `<input checked type=checkbox>` | `Check(white)` |
| `<input checked type=radio>` | `Dot(white)` |
| `<img>` with a decoded picture | `Image { rgba, width, height }` |
| element with `background-image: url(...)` and a decoded picture | `Background { rgba, width, height, size, repeat, position, scale: zoom / picture.scale }` |
| `<progress>`, `<meter>`, `<input type=range>` with a value | `Bar`s: see below |
| the focused text field | selection (`rgba(70,130,255,110)`), IME underline, caret (`Bar`s) |

*Progress/meter*: one bar at the border-box corner offset by the border, width proportional to the fraction, accent `rgb(0,117,255)`. *Slider*: a 4-px (x scale) grey track `rgb(196,196,196)`, the same track in the accent colour up to the fraction, and a thumb 10 x scale wide, full height minus 4 x scale. *Fraction* = `(now - min) / (max - min)` (0 when `max <= min`); an indeterminate progress bar (no value) draws nothing.

*Field decorations* (`field_decorations`): the caret is a 1-px bar of the text colour and the font's ascent+descent high; it is drawn only when `caret_on` is true. For an empty field it stands at the content-box origin, vertically centred for one-line fields and at the top for `textarea`. While composing, the pre-edit text is underlined with a 1-px bar one pixel below the baseline, the caret goes to the pre-edit cursor, and the selection is not drawn over it. The selection highlight is drawn per text fragment.

*Page background*: the root's computed background if its alpha is non-zero, else the first child's (`<body>`), else white; `canvas.clear(...)` fills the whole canvas with it before painting.

## layout

```rust
#[must_use]
pub fn layout(&self) -> std::rc::Rc<quench_layout::LayoutBox>
```

Runs style and layout alone: resolves the images the document asks for that are not yet known (`<img src>`; a failure is remembered as `None` and not retried), builds the cascade if needed, starts an animation frame (`Anim::begin(reduced)`), builds the node tree from node 0, lays it out for `viewport`, stores the result as the current layout (what pointer events are matched against) and clears the `stale` and `restyle` flags. **Panics** only if the document has no root (cannot happen with a `NativeRenderer`). Used by hosts that need the boxes without painting, and by the accessibility and input code (indirectly).

## render

```rust
pub fn render<S: AsRef<[u8]> + AsMut<[u8]>>(&self, canvas: &mut quench_raster::Canvas<S>) -> std::rc::Rc<quench_layout::LayoutBox>
```

Draws the document into `canvas` and returns the layout used. Steps:

1. Takes the layout for the frame: the cached one when nothing is stale, no restyle is pending and no animation is active; otherwise runs [`layout`](#layout). So a scroll or a caret blink repaint reuses the last layout; a hover, focus, press, key, document change or running animation lays out again.
2. Loads and decodes the pictures that boxes ask for with `background-image` (`url`s found in the layout).
3. Clears the canvas to the page background, paints with the decorations above, applying `ui.scroll` offsets.

The layout is made for the viewport that was set, not for the canvas size. Returns the `Rc<LayoutBox>`; the same one is kept for hit-testing. Drawing does not clear `dirty`; call [`take_dirty`](api-input.md#take_dirty).

## Images

* `<img src>` and `background-image: url(...)` accept a `data:` URL (base64 when the header ends with `;base64`, else `%XX`-escaped text), the bytes registered with `add_image`, or the loader's bytes.
* Decoding: `quench_image::decode` (PNG, JPEG, GIF, WebP as `quench-image` supports); on `Unsupported("SVG"|"unknown")` the bytes are treated as UTF-8 SVG text and drawn by `quench_svg::render(text, zoom)` at the page density, so drawings are redrawn when the scale changes. A bitmap has `scale` 1.
* Failures are cached as `None` per `src`; `set_image_loader` clears the failures.
* `background-image` painting: over the padding box, after the colour and before the border; `background-size` (`auto`, `cover`, `contain`), `background-position`, `background-repeat` from the computed style; gradients are not drawn; a background is not clipped to rounded corners. *Unverified:* these limits come from the quench-paint side and the previous docs; they are not enforced in this crate.

## CSS the renderer itself reads

All CSS parsing and cascading is [quench-css](../quench-css/index.md), box geometry is [quench-layout](../quench-layout/index.md), drawing is [quench-paint](../quench-paint/index.md). Fields of `ComputedStyle` this crate reads or writes directly:

| Field (CSS) | Used for |
|---|---|
| `display` | text nodes forced `Inline`; `img` `Inline` -> `InlineBlock`; controls `InlineBlock`/`Block` -> `InlineFlex`/`Flex`; hidden test in accessibility (`Display::None`) |
| `flex_direction`, `justify_content` | set for buttons/text fields (column, centre) |
| `height`, `min_height`, `padding`, `border_width`, `border_box` | minimum height of empty inputs and textareas |
| `font_size` | root font size for `rem`/`em` |
| `color` | placeholder, caret colour |
| `background`, `background_image`, `background_size`, `background_repeat`, `background_position` | page background, image decorations |
| `overflow` | scroll containers (`auto`, `scroll`), hit-test clipping |
| `position` (`fixed`) | hit-testing, accessibility bounds, scroll origin |
| `transform.translate` | hit-testing and accessibility bounds (only the translate part is applied here) |
| `visible` (`visibility`) | accessibility hiding |
| `touch_action` | `touch_action_at` |
| `transitions`, `animations`, `opacity`, `border_color` | [animations](api-anim.md) |
| `media` queries, `env(safe-area-inset-*)`, `zoom` | via `Cascade` configuration |

Selectors the DOM answers: type, `#id`, `.class`, any attribute, descendant/child/sibling combinators supported by quench-css, and pseudo-classes `:disabled`, `:checked`, `:hover`, `:active`, `:focus`, `:focus-visible` (the `State` enum above). Structural pseudo-classes beyond `previous_sibling` support depend on quench-css. *Unverified:* the full property and selector list lives in the quench-css pages.

## Fonts

The renderer owns a `quench_layout::Fonts` (`Fonts::system()`), used for measuring text, line height (`Fonts::line_height`), glyph metrics (`metrics`) and width (`measure`) when placing carets and hit-testing text.
