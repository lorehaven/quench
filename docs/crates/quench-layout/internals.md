---
title: Internals
parent: quench-layout
nav_order: 4
---

## Structure

`layout` creates an `Engine { fonts }` and lays the root out with a `Constraint { avail_w: viewport.0, avail_h: Some(viewport.1), width: Fill, height: None, collapse: false }`, places the root box at its margin offsets, then `resolve`s out-of-flow boxes recursively.

`Constraint` carries what a parent decides for a child: `avail_w`/`avail_h` (the containing block's content box; percentages and `Fill` refer to them), `width` (`Fill` = take the container's, `Shrink` = content-sized within the container, `Exact(border-box width)` = a flex or grid item), `height` (an exact border-box height from the parent), and `collapse` (whether margins may leave the box).

## `layout_node` (engine.rs)

1. Resolve margins/padding/borders against `avail_w`.
2. Content width from the constraint, the style's `width`, a replaced box's size, `Fill` or `Shrink` (`min(max-content, max(room, min-content))`), then `min-width`/`max-width` unless `Exact`.
3. Auto horizontal margins distribute the free space (only for a block-level fill/definite width).
4. Definite height: from the constraint, `height` (px, or `%` when `avail_h` is known), or a replaced box's proportions.
5. Content flow: `flex_flow`, `grid_flow` or `block_flow` (text is wrapped as a one-child block).
6. Height = definite height or the flow's; `min-height`/`max-height` applied if the parent did not force a height. Escaped margins of the first/last child are collapsed into this box's margins.
7. Child offsets by the border+padding; `overflow` computed from children (a clipping child contributes only its own size) and fragments.

## Block flow

`groups()` splits children into `Inline(run)`, `Block(node)` and `OutOfFlow(node)`. A cursor and a pending bottom margin implement collapsing (`collapse(a, b)`). Relative offsets are applied to the stored position, not the cursor. Out-of-flow boxes become `Pending { node, x, y }` with their static position.

## Inline flow

Tokens: `Word`, `Space`, `Open`/`Close` of an inline element (with its horizontal margin+padding+border), `Atom` (a laid-out box), `Break`, `OutOfFlow`. A greedy line builder adds tokens; a word or atom that would exceed the width (by more than 0.01) starts a new line when the line already has content. `finish_line` removes a trailing space, computes the line's ascent/descent from the strut and every part, shifts by `text-align`, emits `InlineFragment`s for open elements (they continue on the next line with `first = false`) and then `TextFragment`s and atoms. Adjacent text pieces of the same node and style are merged into one fragment.

## Intrinsic sizes (intrinsic.rs)

`intrinsic_content` returns `(min-content, max-content)`: text: longest word / widest line between breaks; block: max over children; flex row: sum of items (min = sum, or the largest when wrapping; plus gaps); flex column: max of items; grid: per-track sums via `grid_intrinsic` (spanning items share their size equally over the tracks); replaced: its width. `intrinsic_outer` adds padding, border and margin and honours a definite `width`, `min-width`, `max-width` in px.

## Flex

Items are resolved (`flex_item`): main margins, flex-basis (`flex-basis` else definite main size else content size), min/max, hypothetical size. Items are split into lines (if wrapping) by hypothetical size plus gaps. Each line runs the freeze loop. Items are laid out at their target main size to learn the cross size; lines get cross sizes (the single line takes the container's inner cross size when definite); stretched items are laid out again at the line's size; then main-axis distribution and cross alignment. The reversed directions reverse the item list and swap `start`/`end` of `justify-content`.

## Grid

`grid_place` (placement), `size_tracks` (sizing), `grid_flow` (assembling). Track sizing for a given available size: fixed lengths first; flexible tracks (`fr` or `minmax(_, fr)`) get `floor = max(minmax minimum, content min)`; the remaining room divided by the sum of `fr` (at least 1, so less than one `fr` in total leaves room unused); tracks that would fall under their floor are fixed at it and the loop repeats (at most `flexible + 1` times).

## Positioned boxes (position.rs)

After the in-flow pass, `resolve` walks the tree with each box's page origin and the nearest positioned ancestor's padding rectangle. For each pending box it lays the box out (`layout_positioned`) against that rectangle, converts the result to coordinates relative to its parent, and recurses.

## Fonts

`Fonts::space` is measured as `measure("x x") - measure("xx")` and cached by (font size bits, face index). Layout caches in `quench-text` (`FontSet::layout`) make repeated measuring of the same word cheap.
