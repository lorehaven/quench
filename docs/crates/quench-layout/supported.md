---
title: Supported layout
parent: quench-layout
nav_order: 3
---

What the engine reads from `ComputedStyle` (the CSS field name is given; how each value is parsed is on the quench-css [supported page](../quench-css/supported.md)). Anything not listed is not read by layout.

## Display modes

| `display` | Behaviour |
|---|---|
| `none` | No box and no room; its children are not laid out. (Text nodes are never hidden by this.) |
| `block` | Block box: fills the container, stacks vertically. |
| `list-item` | A block. No marker is laid out. Margins collapse as for `block`. |
| `inline` | Inline element with children: its children flow into the parent's lines; padding, border and margin at left/right take horizontal room; reported as `Fragment::Inline` per line. An `inline` node whose content is not `Children` (text) is just text. Vertical padding/margin do not change line height. |
| `inline-block`, `inline-flex`, `inline-grid` | Atomic inline: laid out shrink-to-fit, placed on the line by baseline, wrapped as a unit. |
| `flex`, `grid` | Block-level flex / grid container. |

Out-of-flow (`position: absolute | fixed`) nodes are taken out of every container (block, flex, grid, inline) and placed afterwards.

## Properties read

| Property (field) | Honoured as |
|---|---|
| `width`, `height` | `Length` (`px`, `%`, `calc()`, etc.). `%` of width against the containing block's content width; `%` of height only when the containing height is known (otherwise treated as auto). `auto` = `None`. |
| `min-width`, `max-width` | Applied to block and inline-block width (not to a flex item whose width is forced by the flex algorithm; flex uses them as min/max of the main size). |
| `min-height`, `max-height` | Applied when height is not forced by the parent; `%` only with a known container height. |
| `box-sizing` | `border-box` or content-box (`border_box`). Applies to width, height, min/max, flex-basis. |
| `margin-*` | `auto` supported: horizontally centres / pushes in block (when the width is definite), on both axes in flex. Vertical auto margins in block flow are 0. Percentages resolve against the containing block **width** (all four sides). |
| `padding-*` | Percentages against the containing block width. Negative values clamp to 0. |
| `border-*-width` | Adds to the box (`border_width`); style/colour are painter's concern. |
| `overflow` | Only its effect on layout: a clipping box does not pass its content's extent to the parent's `overflow`, does not collapse margins with its children, and a flex item that clips has min size 0. Scrolling itself is the host's. |
| `position` | `static`, `relative`, `absolute`, `fixed`. (`sticky` is parsed by quench-css as `relative`.) |
| `top/right/bottom/left` | `relative`: `left` wins over `right`, `top` over `bottom`. `absolute/fixed`: see below. |
| `z-index` | Not used by layout (the painter orders). |
| `flex-direction` | all four. |
| `flex-wrap` | `wrap` / `nowrap` (`wrap-reverse` is treated as `wrap`: the field is a `bool`). |
| `flex-grow`, `flex-shrink`, `flex-basis` | Full flexible-length algorithm. |
| `justify-content` | `flex-start`/`start`, `flex-end`/`end`, `center`, `space-between`, `space-around`, `space-evenly` (the six `Justify` variants). |
| `align-items`, `align-self` | `stretch`, `start`, `end`, `center`, `baseline` (baseline only in rows; in a column it behaves as start). |
| `gap`, `row-gap`, `column-gap` | Flex and grid. Percentages: column gap against the width, row gap against the container height (0 if unknown). |
| `grid-template-columns/-rows`, `grid-auto-columns/-rows`, `grid-column`, `grid-row` (+ start/end longhands), `grid-template-areas`, `grid-area` | see Grid below. |
| `text-align` | `start`/`left`, `end`/`right`, `center`; per line in a block's inline content. No `justify`. |
| `text-transform` | `uppercase`, `lowercase`, `capitalize` (first alphabetic character after whitespace); applied before measuring. |
| `line-height` | `normal`, number, px. |
| `font-size`, `font-weight` | measuring (weight chooses the face); `font-family` and `italic` do not change measuring. |

Not read by layout: `float`, `clear`, `display: table*`, `columns`, `white-space` (collapsing is always on), `vertical-align` (always baseline), `order`, `align-content` (see flex), `justify-items`, `justify-self`, `grid-auto-flow`, subgrid, list markers, `object-fit`, `aspect-ratio`, `writing-mode`, bidirectional text, hyphenation. Most of these properties are not recognised by quench-css either.

## Block flow

* Children are grouped: runs of inline-level children (text, `inline*`) become one inline formatting context; block children each get a box; a run containing only whitespace text is dropped.
* Width: `Fill` (container width - margins - padding/border) unless `width` given. Replaced boxes use their size; with only one side given, the other follows the proportions.
* **Margin collapsing**: adjoining sibling margins collapse (both >= 0: the larger; both <= 0: the most negative; mixed: sum). A first child's top margin and a last child's bottom margin pass through the parent when the parent is `block`/`list-item`, has no padding or border on that side, has `overflow: visible`, is not out of flow, was not laid out as a flex item / inline block / grid item, and (for the bottom) has no definite height. Margins around an inline formatting context do not collapse through it.
* Percent padding and margin use the containing block width.

## Inline flow

* Text is split into words at Unicode whitespace; runs of whitespace collapse to one space; leading spaces on a line and trailing spaces are dropped.
* A word wider than the line is not broken: it overflows (tolerance 0.01 px).
* Line height: the tallest of the block's strut (its own font and line-height, half-leading) and every text run's / inline-block's extent above and below the baseline; metrics are whole pixels.
* Atoms (inline-blocks, inline flex/grid, replaced boxes) sit on the baseline of their first line and take their horizontal margins; vertical margins extend the line.
* `LINE_BREAK` (U+2028) forces a new line.

## Flexbox

Row, row-reverse, column, column-reverse; single-line and wrapping; the flexible-length steps (hypothetical sizes, freeze on zero factor or on min/max violation, loop at most `items + 1` times); `gap`; main-axis `justify-content`; cross-axis `align-items` / `align-self`; auto margins; baseline alignment (rows). The automatic minimum size of an item is its min-content width (rows) / content height (columns), 0 when the item clips on that axis; a definite `width` caps it. Items that are text become anonymous block items (key `usize::MAX`); whitespace-only text is dropped; `display: none` and out-of-flow children are not items. Leftover cross space with several lines is shared equally between lines (an `align-content: stretch` behaviour; the property is not read). A single-line row container with no definite height takes `min-height`/`max-height` into account and centres items inside. The `order` property is not supported.

## Grid

* Tracks: `<length>`, `<percentage>`, `fr`, `auto`/`min-content`/`max-content` (all three are `Track::Auto`: sized by content min/max), `minmax(a, b)`, `repeat(n, ...)`, `repeat(auto-fill | auto-fit, ...)` (as many repeats as fit; `auto-fit` is **not** collapsed, it behaves as `auto-fill`). Implicit tracks use `grid-auto-columns`/`grid-auto-rows` (default `auto`).
* Placement: `grid-column`/`grid-row` as line numbers (negative from the end), `span n`, `a / b`; `grid-area: name` against `grid-template-areas`; a name the template lacks is placed by flow. Items with both row and column fixed are placed first; others flow row by row into the first free cell (sparse; no `dense`, no `grid-auto-flow: column`). Text directly in a grid becomes an anonymous item.
* Sizing: columns from the span-1 items' min/max content, then `fr` tracks share the remaining width (never below their content minimum); content-sized tracks shrink to their minimum when too wide and share leftover room when there is some. An item spanning several columns widens the **last** column it crosses when needed; rows likewise. Rows stretch to a given container height.
* Alignment: items stretch across their area unless they have a size; `align-items`/`align-self` (`stretch`, `start`, `end`, `center`; `baseline` behaves as start) place them vertically. There is no horizontal alignment (`justify-*` not read).
* `inline-grid` participates as an atomic inline.

## Positioning

* `relative`: offset after layout; `left` over `right`, `top` over `bottom`; percentages against the parent width / height (0 if unknown); neighbours do not move.
* `absolute`: containing block is the padding box of the nearest ancestor with `position != static`, else the viewport rectangle. Width: with `width: auto` and both `left` and `right` set, `containing width - left - right - margins`; otherwise shrink-to-fit. Height likewise with `top`/`bottom`. Position: `left` (else `right`, else the static position) plus the margin. Insets resolve against the containing block (`%` too).
* `fixed`: containing block is the viewport. The painter undoes scrolling for it.
* Out-of-flow boxes are appended to the `children` of the box that contains them in the tree (not of the containing block), with coordinates relative to that parent, and extend its `overflow`.

## Known limits

* No incremental layout; no caching between calls to `layout`.
* Grid and flex heights with `%` need a definite container height.
* Fonts: only weight selects a face. See [quench-text](../quench-text/internals.md).
