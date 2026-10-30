---
title: Supported SVG
parent: quench-svg
nav_order: 3
---

Everything below was read from `src/lib.rs` and `src/xml.rs`.

## XML reader

* Elements, attributes (single- or double-quoted), text (ignored), comments, CDATA, processing instructions/declarations (`<?...?>`, skipped), doctype (skipped, bracketed internal subset included; **entities declared in it are not read**).
* Entities in attribute values: `&amp; &lt; &gt; &quot; &apos;` and numeric references (`&#52;`, `&#x34;`). An unknown entity such as `&bogus;` is left as is.
* **Namespaces are ignored**: a prefix is dropped from element and attribute names (`svg:rect` is `rect`, `xlink:href` is `href`). `xmlns` attributes have no effect.
* Nesting deeper than 64 is `Invalid("elements are nested too deep")`.

## Elements drawn

| Element | Handling |
|---|---|
| `svg` (root) | size and `viewBox` (see [API](api.md)); its own `fill`, `stroke`... apply to children |
| `svg` (nested) | children drawn translated by `x`, `y`; its own `viewBox` and size are **not** applied |
| `g`, `a`, `switch` | children drawn in order (all children of `switch` are drawn; no conditional evaluation) |
| `use` | `href` (or `xlink:href`) of the form `#id` referencing an element anywhere in the document (found by depth-first search for the first `id`); translated by `x`, `y`; recursion depth limit 8; a missing target draws nothing |
| `rect` | `x y width height rx ry`; missing `rx`/`ry` copies the other; zero or negative size draws nothing |
| `circle` | `cx cy r` (r <= 0 draws nothing) |
| `ellipse` | `cx cy rx ry` (either <= 0 draws nothing) |
| `line` | `x1 y1 x2 y2` |
| `polyline`, `polygon` | `points` pairs (an odd trailing number is dropped); `polygon` closes |
| `path` | `d`: `M m L l H h V v C c S s Q q T t A a Z z`; implicit repeats, compact number syntax (`1.5.5`, `-1-2`), arc flags without separators |

Curves and ellipses are flattened to polylines (circles/ellipses use `clamp(ceil(3 * r), 8, 48)` segments).

Not drawn (skipped silently): `text`, `tspan`, `image`, `defs` content (only reachable through `use` and `url(#id)`), `symbol`, `pattern`, `linearGradient`/`radialGradient` as such, `filter`, `mask`, `clipPath`, `marker`, `style` (CSS in `<style>` is not read), `title`, `desc`, `animate*`.

An element with `display="none"` (attribute or in `style`) or `visibility="hidden"` (attribute) is skipped together with its subtree. `visibility` in `style` is not read. At most 100 000 elements are visited and recursion deeper than 64 is cut off; both limits stop drawing without an error.

## Properties

Read from attributes, then from the `style` attribute (`name: value; ...`), in that order; inherited down the tree. `color` is applied first so `currentColor` in the same element sees it.

| Property | Values | Default |
|---|---|---|
| `fill` | `none`, a colour, `currentColor`, `url(#id)` | black |
| `stroke` | same | none |
| `color` | a colour | black |
| `stroke-width` | length (percent relative to 100), clamped >= 0 | 1 |
| `fill-opacity`, `stroke-opacity` | number (unparsable -> 1) | 1 |
| `opacity` | number clamped 0..1, **multiplied** down the tree (not inherited as a group effect) | 1 |
| `stroke-linecap` | `butt` `round` `square` | `butt` |
| `stroke-linejoin` | `miter` `round` `bevel` | `miter` |
| `stroke-miterlimit` | number | 4 |
| `transform` | `matrix(6)`, `translate(1 or 2)`, `scale(1 or 2)`, `rotate(a)` or `rotate(a cx cy)`, `skewX`, `skewY`; a list; unknown or malformed items are identity | identity |

Colours go through `quench_css::parse_color` (CSS notations such as hex, `rgb()`, named colours, `currentColor`; see [quench-css](../quench-css/index.md)). `url(#id)` paints with the **first `stop`'s `stop-color`** of the referenced element (read from the `stop-color` attribute or `style`, default black). Fills use the non-zero winding rule only (`fill-rule` is ignored). Strokes are built as quadrilaterals per segment with joins and caps (`dash` properties ignored). Group `opacity` is applied per child, so overlaps inside a group show through.
