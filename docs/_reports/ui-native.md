# Gap report: quench-ui-native

## 1. Stale or wrong statements in the old docs (`docs/quench-ui-native.md`, `src/anim.rs`)

* `src/anim.rs:7` (module doc): "Keyframe animations are not played." Wrong: `Anim::play` plays them (and the old page and tests say so).
* Old page: "`:hover`, `:focus` and `:active` are false until slice 3 gives the renderer a pointer." Stale; the states are real (`pipeline.rs:66-77`).
* Old page: "a thin bar shows where in a scroll container (or the page) you are". Not in this crate; scrollbar drawing is only in `quench-paint/src/lib.rs`, not documented here as verified.
* Old page: "`add_image(src, bytes)`" described as bytes registered; the rustdoc says "(a PNG)" but `picture()` accepts any `quench-image` format and SVG text (`pipeline.rs:388-404`).
* Old page links to `../plans/NATIVE_RENDERER.md`, "slice" language, "Next steps: other platforms" (X11, Windows, macOS, multi-window): planning prose, not behaviour; dropped.
* Old page: "`set_media` (... the size follows the viewport)": true, but also the width/height fields passed in are overwritten (`dom.rs:301-305`).

## 2. In code, undocumented before

* All of `pointer_drag`, `pointer_release`, `pointer_cancel`, `touch_action_at`, `element_at`, `set_caret_visible`, `scroll_of`, `focused`, `text_field_focused`, `set_field_value`, `kind/attrs/styles/children/text_of/tag/attr` on `NativeRenderer` (some were named, none specified).
* Exact key tables (editing, slider, scroll keys), `Role` mapping tables, accessible-name order, hidden rules, inline-like tag list.
* Button/field vertical centring uses an anonymous box with key `usize::MAX`.
* Placeholder colour `rgb(117,117,117)`, slider/progress colours, selection colour `rgba(70,130,255,110)`.
* `perform(ScrollIntoView)` implementation (wheel at 0,0).
* Radio groups are document-wide; label click forwarding rules; submit-button rules.
* `NativeRenderer::new()` replaces the thread's `quench_ui::platform`.
* `set_scale` clamp (min 0.1).
* Events never produced: `DoubleClick`, `Change`, `TransitionEnd`, `Resize`.

## 3. Open questions / suspected bugs

* `access.rs:115`: `accessibility_tree` re-lays out when `ui.dirty` is set, not when `ui.stale` is. A host that calls `take_dirty()` (clears `dirty`, leaves `stale`) after a document change and before `render` gets a tree from the old layout. `input.rs:31-39` (`ensure_layout`) checks `stale` instead.
* `dom.rs:130-135`/`504-514`: removed nodes are never freed; scans over all nodes (`input.rs:328-334` radio group, `input.rs:1180` label `for`, `access.rs:395,455,465` id lookups, `pipeline.rs:472` image resolution) include detached nodes. Memory and correctness (a detached radio/id) concerns.
* `input.rs:328`: radio groups are document-wide, not scoped to the form.
* `access.rs:543`: a `password` field's real value is exposed as `AccessNode::value`.
* `access.rs:518`: `aria-readonly` is ignored (only `readonly`).
* `input.rs:1003-1013` and `1203-1220`: `place_caret` ignores `y`, and `caret_index` only uses the first text fragment, so clicks in later lines of a multi-line `textarea` land on the first line (not tested).
* `input.rs:532`: `scroll_key(_, _ctrl)` ignores its `ctrl` argument.
* `input.rs:1094-1096`: `tabindex` negative means not focusable at all (a browser keeps it click-focusable); positive `tabindex` order not honoured.
* `timers.rs:150-162`: a channel can never be removed; `next_wake` then wakes every 25 ms forever.
* `dom.rs:231-232`: a second renderer on a thread silently takes over the platform of the first.
* `anim.rs` state (`last`, `running`, `started`) is never pruned for removed nodes.
* `input.rs:589` has a doc comment ("Replaces the selection...") on `select_word_at` left from `replace_selection` (rustdoc is wrong for both).
* `pipeline.rs:103-135`: date/time display is US-only (`mm/dd/yyyy`, AM/PM), no locale.
* `native-measure.mjs` purpose not documented anywhere.
