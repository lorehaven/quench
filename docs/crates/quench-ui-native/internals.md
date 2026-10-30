---
title: Internals
parent: quench-ui-native
nav_order: 8
---

Design notes that matter to users. Source: `src/dom.rs`, `src/pipeline.rs`, `src/input.rs`.

## Node arena

All nodes live in one `Vec<DomNode>` (`kind`, `parent: Option<usize>`, `children: Vec<usize>`). `NodeId`, layout box keys and accessibility ids are indexes into it. `<html>` is 0, `<body>` is 1. Node kinds: `Element { tag, attrs, handlers, styles }`, `Text`, `Anchor`.

* `remove` only detaches. A removed node (and its subtree) stays in the arena: the arena never shrinks and indexes are never reused. Consequences: memory grows with the number of nodes ever created; whole-arena scans (`<img>` resolution, label `for` / `aria-labelledby` id lookup, radio groups in `toggle`, `label_target`) also see detached nodes. A detached radio with the same `name` has its `checked` cleared by a click; a detached element with a matching `id` can be found as a label target. See the [report](../../_reports/ui-native.md).
* Animation state (`last`, `running`, `started`) is keyed by node index and is not cleaned when a node is removed.
* Layout key `usize::MAX` marks the anonymous block that centres button/field content; it is skipped by hit testing and accessibility bounds.

## Interaction state

`Interaction` holds: `hovered`, `active`, `focused`, `focus_visible`, `trap`, `scroll` (offsets by box key), `pressed`, `caret`/`anchor` (character indexes into the focused field's value), the internal clipboard and its export flag, the IME composition `(text, cursor in characters)`, and the flags below. Caret and anchor are reset to the end of the value on every focus change.

## Dirty flags and layout reuse

Three flags decide what a frame costs:

| Flag | Set by | Meaning | Cleared by |
|---|---|---|---|
| `dirty` | every mutation, `touch()`, scroll, caret blink, `tick` | something on screen is out of date (what `take_dirty` reads) | `take_dirty` |
| `stale` | document, sheet, viewport, scale, media, safe-area or image changes; `tick` that ran something | the layout is out of date | `layout()` |
| `restyle` | `touch()`: everything that may change a style or size (document, hover, focus, press, keys, dispatched events except `Scroll`) | a new layout is needed | `layout()` |

`render` reuses the cached `Rc<LayoutBox>` when `!stale && !restyle && !anim.active()`. Scrolling and caret blink set only `dirty`, so a frame repaints with the old layout: laying out the whole document (cascade, every box, every line) for each frame of a scroll makes long lists crawl. Input methods call `ensure_layout` (layout if `layout.is_none() || stale`) before hit testing, so events always match the current document; the effect of `restyle` alone (hover) shows at the next `render`.

Cascade cache: `Option<Rc<Cascade>>` rebuilt on the next layout after any sheet/viewport/scale/media/safe-area change; element styles are recomputed on every layout (nothing else is cached between layouts).

## Units and scale

The viewport, pointer positions, safe area, scroll offsets and the layout are in device pixels. CSS lengths are multiplied by the scale through the cascade (`cascade.zoom`); `media.width/height` are `viewport / scale`. `img` natural sizes are `pixels / picture.scale` CSS pixels, then multiplied by the scale; SVG pictures are rendered at the scale and re-rendered when it changes. Slider and progress decorations multiply their constants (4, 10, 2 px) by the scale.

## Text fields

* The value is the `value` attribute (a `textarea` without the attribute reads its own text children). Editing writes the attribute; `Input` carries the new value. The caret is an index in characters; `ImeState` converts to bytes.
* Caret placement from a click picks the character whose prefix width (`measure(prefix + "|") - measure("|")`, so a trailing space counts) is nearest to `x` in the first text fragment of the field. *Unverified:* multi-line `textarea` clicks use only the first fragment (`caret_index` reads `frags.first()` and ignores `y`), see report.
* `place_caret` ignores `y`.
* The composition (`preedit`) is shown by splicing its text into the displayed value at the selection start; it is not in the attribute.

## Hit testing and stacking

Hit testing mirrors painting order via `quench_paint::Layers::of(box)` (above, inline, blocks, below). See [input](api-input.md#events-and-default-actions) for the exact order.

## Events not produced

`DoubleClick`, `Change`, `TransitionEnd`, `Resize` exist in `quench_ui::Event` but this renderer never delivers them. Listeners registered for them never fire.

## Platform and threads

Everything is single-threaded (`Rc`, `RefCell`). Cross-thread data goes through `channel`. The `quench_ui::platform` registration is thread-local: one active renderer per thread.

## Performance

*Unverified (from the previous docs, measured on one machine):* a full frame of the component gallery took about 130 ms in a debug build; nothing is cached between layouts beyond the cascade. The unit test `a_scroll_or_a_blink_reuses_the_layout_but_a_hover_or_a_change_makes_a_new_one` guards the reuse rules.

## Known simplifications

* No table layout, no `<select>` popup/value, no `contenteditable`, no `input type=color|file` behaviour (empty UA rule), no `<video>`/`<canvas>` content.
* Tab order ignores positive `tabindex`.
* Accessibility roles for tags are a fixed list; `aria-*` support is the list in [api-access](api-access.md).
