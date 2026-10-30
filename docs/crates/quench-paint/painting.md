---
title: Painting
parent: quench-paint
nav_order: 3
---

## Order

For each box (`display: none` boxes are skipped entirely):

1. `transform: translate(...)` shifts the box (percentages of its own width/height). Boxes more than 160 px outside the canvas clip (including their `overflow` reach) are skipped with all descendants.
2. If `opacity` < 1, or the box has `rotate`/`scale`, the box and its descendants are painted into a transparent layer (the box area plus 64 px margin on each side, limited to the canvas), which is composited with the opacity, and rotated/scaled about the middle of the border box (`transform-origin: 50% 50%`). `opacity <= 0` paints nothing. Opacity 1 without rotation/scale paints directly.
3. If visible (`visibility`): outer `box-shadow`s (the last listed first, so the first is on top), `background-color`, `Decoration::Background`, inset shadows, border.
4. If `overflow` is not `visible` on both axes: clip to the padding box (snapped to whole pixels). Scroll offsets come from the `scroll` callback.
5. Children: negative-`z-index` positioned (`below`), in-flow blocks, inline element backgrounds/borders (`Fragment::Inline`), text fragments, inline-blocks/inline-flex, positioned boxes with `z-index >= 0` by z-index.
6. Scroll bar (clipping boxes with `overflow-y: auto | scroll` and overflowing content and padding height >= 12 px; the root box gets a page bar along the right window edge when its content is taller than the canvas), then decorations (`Check`, `Image`, `Dot`, `Bar`), then the clip is restored.
7. Outline (outside the clip).

There are no full stacking contexts: `z-index` orders positioned siblings within their parent only.

## Per property

| Property (`ComputedStyle` field) | Painting |
|---|---|
| `background` colour | `fill_rrect` on the box snapped to whole pixels, with corner radii. Resolved against `color` for `currentcolor`. Alpha 0 draws nothing. |
| `background-image` etc. | Only through `Decoration::Background` supplied by the host (the style stores URL/size/repeat/position; the painter does not load images). |
| `border-*` | Uniform solid same-colour borders: one `stroke_rrect` ring. Otherwise per side strips (top and bottom strips take the corners; left/right fit between): `solid` (with radii: ring clipped to the strip), `dashed` (dash and gap = 3 x width), `dotted` (square dots, dash = gap = width), `none`. |
| `border-radius` | `Length`s resolved against the smaller side; all four scaled down together when two adjacent radii exceed a side. |
| `box-shadow` | Multiple layers, `inset` supported, blur (Gaussian, sigma = blur/2), spread, x/y offset, colour. Shadows are not painted for inline fragments. |
| `outline-*` | Painted after everything, outside the border box grown by `outline-offset + outline-width`; any style other than `none` is drawn solid (dashed/dotted not distinguished); corners rounded when the box has a radius. |
| `opacity`, `transform` | As above. Only `translate`, `scale` and `rotate` exist in `Transform`. |
| `overflow` | Clip to padding box; both axes clipped if either is not `visible`. |
| `visibility: hidden` | Box's own shadow/background/border/outline/decorations/bar are skipped; children still paint if they are visible. |
| `color` | Text colour (`style.color`). |
| `font-weight` | Chooses the face (see quench-layout `Fonts::set`). |
| `font-style: italic` | Regular face leaned by 0.2 (advances unchanged). |
| `font-family` | Ignored. |
| `font-size`, `text-align`, `text-transform`, `line-height` | Already applied by layout (positions and text). |
| `list-style` | Ignored (no markers are painted). |
| `text-decoration`, `text-shadow`, `letter-spacing`, filters, gradients | Not in the style model: not painted. |
| `position: fixed` | Does not follow the scrolling of containers above it. |
| `z-index` | Orders positioned children as above. |
| `cursor`, `touch-action`, transitions, animations | Not painting concerns (the host applies animated values to the style before painting). |

## Text

A text fragment is laid out again with `FontSet::layout(text, font_size, None)` and drawn with `draw_slanted` at `(round(x), round(baseline) - ascent)` in `style.color`. Fragments of `visibility: hidden` text are skipped.

## Scroll bars

6 px wide, rounded, colour `rgba(160, 160, 160, 150)`, 2 px from the right edge, minimum thumb 24 px.
