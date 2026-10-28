# Quench Layout

`quench-layout` (crate `quench_layout`) places boxes with no dependencies: block and inline flow, **flexbox**, positioning and margin collapsing, over the computed styles of [`quench-css`](./quench-css.md) and the text of [`quench-text`](./quench-text.md). It is the fourth piece of the [native renderer](../plans/NATIVE_RENDERER.md).

```rust
use quench_layout::{Fonts, Node, layout};

let fonts = Fonts::system();                         // Noto Sans, the machine's fonts behind it
let root = Node::element(1, root_style, vec![ /* Node::element, Node::text, Node::replaced */ ]);
let boxes = layout(&root, (390.0, 844.0), &fonts);   // a LayoutBox tree
let (x, y) = boxes.position_of(2).unwrap();          // a node's border box from the page's origin
```

A `Node` is a style (`Rc<ComputedStyle>`), a `key` of the caller's own (an index into its DOM; it comes back on the box) and content: children, text, or a `Replaced(width, height)` box with a size of its own (a checkbox, an image). A `LayoutBox` has its border box (`x`, `y`, `width`, `height`, from its parent's border box), the margin, padding and border it resolved, its child boxes, the **fragments** of its inline content, its first baseline and how far its contents reach (for scrolling). `content_rect()` and `padding_rect()` give the inner boxes.

## What is laid out

- **Block flow**: boxes stack, fill their container, take `width`/`height` in `px`, `%`, `calc()` and friends, `min-`/`max-` limits and `box-sizing`; horizontal `auto` margins centre. **Margins collapse**: adjoining siblings (the larger of two, or their sum when the signs differ), and a first or last child's margin through its parent when the parent has no padding, border or `overflow` and is not a flex item or an inline block. Percentages in padding and margins are of the container's width, in heights of its height when that is known.
- **Inline flow**: text and inline elements break into lines at spaces (white space collapses; a word wider than the line overflows it), `text-align` shifts each line, `text-transform` applies, a line is as tall as its tallest part (line height with half-leading, baseline aligned, a strut from the block's own font). Inline blocks, inline flex containers and replaced boxes sit on the line by their baseline and wrap as units. An inline element with padding or a background reports where it is on **each line** (`Fragment::Inline`, with `first`/`last` for the ends). A U+2028 in a text is a line break (the host's `<br>`). Metrics are rounded to whole pixels as browsers do: Noto Sans at 16 px has a 22 px `normal` line.
- **Flexbox**: row and column, reversed, `nowrap` and `wrap`, the full flexible-length algorithm (`flex-grow`, `flex-shrink` weighted by the base size, frozen items, minimum and maximum sizes, the automatic minimum of a flex item is its min-content width unless it clips), `gap`, `justify-content` (all six), `align-items`/`align-self` (stretch, start, end, centre, baseline), auto margins on both axes, `align-content: stretch` for wrapped lines, text directly in a container as an anonymous item (its key is `usize::MAX`). A single row is as tall as `min-height` allows, and its items centre in that, which is what an app bar with `align-items: center` needs.
- **Positioning**: `relative` shifts a box without moving its neighbours; `absolute` and `fixed` leave the flow and are placed against the nearest positioned ancestor's padding box (or the viewport) by their insets, sized by them when both sides are set, else by their content; with no inset on a side the box stays where it would have been. `z-index` is the painter's business.
- **Shrink-to-fit**: inline blocks, inline flex containers, flex items sized by their content and absolutely positioned boxes are as wide as their content wants within the space offered, from min-content and max-content widths worked out over blocks, lines, flex rows and columns.

Not laid out: grid, floats, tables, multi-column, `white-space` other than normal, `vertical-align` other than baseline, `position: sticky` (it is `relative`), bidirectional text, hyphenation, list markers (a `list-item` is a block; the painter has none yet).

## Fonts

`Fonts` holds regular and bold Noto Sans (`embedded()` alone, `system()` with the installed fonts behind them) and gives `metrics`, `measure`, `space` (measured between two letters, since a line's trailing space counts for nothing) and `line_height` for a style. Bold is from weight 600; the two faces are all there are.

## How it is tested

Unit tests build trees from CSS declarations through the real cascade and assert the boxes' rectangles: stacking, margins, text wrapping, every `justify-content`, grow and shrink with frozen items, wrapping, auto margins, baselines, positioning against ancestors and the viewport, shrink-to-fit of every kind of content. The gate that matters is [`scripts/native-gate.mjs`](./quench-ui-native.md#the-gate): the same page laid out here and by Chromium, box by box.
