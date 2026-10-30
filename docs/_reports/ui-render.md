# Gap report: quench-layout, quench-paint, quench-raster, quench-text, guides

## 1. Stale or wrong statements in the old pages

* `quench-layout.md` said "Not laid out: grid, floats, ..." in one paragraph while documenting grid in the previous bullet. Grid (including `grid-template-areas`) is implemented.
* `quench-layout/src/grid.rs` module doc says `grid-template-areas` is "Not built"; the code implements it (`area_of`, `grid_area`) and a test covers it. Same module doc: spanning items "grow the last track" is accurate.
* `quench-layout.md` said percentages in padding and margins resolve "in heights of its height when that is known". The code (`Edges::of`) resolves all four sides against the containing block width.
* `quench-layout.md` credited flex with `align-content: stretch`; the property is not read. Leftover cross space in multi-line containers is divided equally among lines (stretch-like behaviour).
* `quench-layout/Cargo.toml` description and `lib.rs` crate docs mention only "flexbox"; grid exists.
* `quench-text.md` listed two embedded faces (Regular, Bold); there are four (Light, Regular, Medium, Bold) with `noto_sans_light`/`noto_sans_medium` functions and FontSet constructors. Weight mapping: <=350 light, 351-449 regular, 450-599 medium, >=600 bold (layout `Fonts::set`).
* `quench-text.md` stated test counts (17 tests) that no longer match (18 `quench_test::test` functions); coverage figures (94%, 99.5%) were not re-measured and are not repeated.
* `quench-raster.md` said 13 tests (now 25) and omitted `Canvas::over`, `draw_image`, `draw_layer_transformed`, `clip`, `bytes_mut`, `Path::contours`, `Path::bounds`, `Rounded`, and the generic `Canvas<S>`.
* `quench-paint.md` was a stub of the pipeline; `Decoration`, `Layers`, `SCROLLBAR`, `to_raster`, `Scroll`, `Decorations` were not documented.

## 2. Existing in code, undocumented before

* `Fonts::set/space/line_height/metrics`, `Metrics`, `LINE_BREAK` details; `LayoutBox::text_fragments/find/position_of/content_rect/padding_rect`.
* Anonymous flex/grid items for direct text have `key == usize::MAX`.
* `SystemFonts` limits: depth 6, 4096 files, 64 tables, 4 MiB cmap, `emoji` file names skipped.
* `FontSet` layout cache (4096 entries) and character-resolution cache.
* Paint culling margin 160 px, layer margin 64 px, scroll bar geometry, 256x256 tile cap for repeated backgrounds.

## 3. Open questions and suspected bugs

* `quench-layout/src/flex.rs` (reverse handling near "order.reverse(); order.reverse();"): a no-op pair of reversals; with `flex-direction: *-reverse` plus wrapping the whole list of items is reversed, so lines also appear in reverse cross order. CSS keeps cross order. Needs a decision.
* `flex.rs`: `flex_wrap` is a `bool` in `ComputedStyle` (quench-css `style.rs:1666`: anything but `nowrap` is wrap), so `wrap-reverse` behaves as `wrap`.
* `grid.rs` `expand`: `auto-fit` flag is ignored (empty tracks are not collapsed), so `auto-fit` equals `auto-fill`.
* `engine.rs` `layout_node`: for `Width::Shrink` the width is `max.min(room.max(min))`; a negative room is clamped by `max(min)`; fine, but percent heights without a known container height silently become auto.
* `quench-paint` `Layers::of`: `display: inline-grid` boxes go to `blocks` rather than `inline`, so they paint in the block layer (before text), unlike `inline-block`/`inline-flex` (`lib.rs` ~107).
* `quench-paint`: `Decoration::Bar` x/y are added to the unsnapped rect origin `rect.0` after `snap` (consistent), but text is rounded separately; sub-pixel differences between caret and glyphs are possible. Not verified.
* `quench-raster` `Canvas::clear` ignores the clip (writes every pixel); documented, but other drawing respects it.
* `quench-raster` `draw_layer_transformed`: pixels with interpolated alpha <= 0.5/255 are skipped (threshold `mix[3] <= 0.5`), which cuts very faint edges.
* `quench-text` `Font::parse` comment mentions "or the first font of a collection is not read here"; collections are rejected as `NotTrueType`.
* Not run: the browser snippet of the guide; the rest of the guide code was compiled and run in a scratch project (quench-ui with `TestRenderer`).
