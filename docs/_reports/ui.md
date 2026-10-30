# Gap report: quench-ui (core part)

Covers `reactive`, `view`, `event`, `renderer`, `mount`, `html`, `test_renderer`, `virtual_list` and the crate root. Router, form, i18n, kit, platform, anim and gesture are in the other report.

## 1. Stale or wrong statements in the old `docs/quench-ui.md`

* "Line coverage 99%": not re-measured; removed (marked unverified in testing.md).
* "the web backend (`quench-ui-web`, Stage 4)": stage wording is obsolete; the backend exists.
* "Elements ... `Element::css` ... a server render leaves the property out": correct, kept.
* "An effect that keeps triggering itself or another stops after 100,000 runs": the code trips on the 100,000th queue entry of one flush (99,999 processed) and counts runs per flush, not per effect.
* Implied that `Mount` drop only leaves the DOM as it is; in fact a dropped `Mount` that has no parent scope also stops all its effects (verified by a scratch test: text stays stale after `drop(mount)`).
* "Invalid tag names panic": correct; the old text never said that `element()` also lower-cases the tag and accepts custom-element names with `-`.
* "Attribute names that start with `on` ... are dropped": also drops innocent names such as `once`, `online`.
* Old page did not mention the `Key` conversions are limited (no `isize`, `i128`, `u128`).

## 2. Code that was undocumented

* Every `Signal`/`Memo`/`Effect`/`Scope` method (`with_untracked`, `get_untracked` on memo, `Scope::new_in`, `reset`, `current`, `is_disposed`), `render_to_string` semantics.
* `View::lazy`, `Element::focus_if`, `focus_on_mount`, `trap_focus`, `prevent_default`, `intercept_click`, `children`, `attr_opt`.
* `html` module in full (`escape_*`, `is_valid_*`, `sanitize_attr_value`, `VOID_TAGS`, `BLOCKED_URL`).
* `TestRenderer` methods: `outer_html`, `text_of`, `children`, `parent`, `tag`, `style_of`, `clear_ops`, `document_order`; `Op` variants; `NodeId`.
* `EventKind::dom_name` table, `ScrollEvent`, `Size`, `take_prevent_request`.
* `virtual_list::OVERSCAN`, its DOM structure and the assumed 640 px viewport.
* Pub item counts documented: reactive 7 free functions + Signal 7 methods + Memo 3 + Effect 2 + Scope 7 (+ trait impls); view: Key (11 From impls) + View (5 constructors, 7 variants) + IntoView (15 impls) + `fragment`, `element`, 25 tag helpers, 31 Element methods; event 6 types + 3 fns; renderer 2 types, 13 trait items; mount 3 items; html 2 consts + 7 fns; test_renderer 3 types + 19 methods; virtual_list 2 items; crate root `render_to_string`. About 190 items in total.

## 3. Open questions and suspected bugs

* `quench-ui/src/view.rs:63` `KeyedItem` is `pub(crate)` but appears in the public `View::Keyed` variant, so users cannot build `View::Keyed` by hand (probably intended; the alias leaks through the signature).
* `quench-ui/src/mount.rs` (`Mount` has no `Drop` impl): dropping a `Mount` outside any scope silently kills reactivity; docs say "releases handles". Consider a `#[must_use]` on `mount` (it has none) or documenting; it is not documented in the source.
* `quench-ui/src/reactive.rs:296-306` (`flush`): the limit counts queue entries per flush, so a legitimate flush of >= 100,000 effect runs panics. Also, when a flush panics, entries left in the queue stay queued for the next flush (not cleared).
* `quench-ui/src/reactive.rs` `Memo::with` after dispose returns the last value without recomputation (by design?).
* `quench-ui/src/html.rs:99` `is_valid_attr_name` rejects any name starting with `on` (`once`, `online`, `only`); intended as a conservative rule, but `data-*`/`aria-*` are fine.
* `quench-ui/src/html.rs` `sanitize_attr_value` lets every `data:image/...` through, including `data:image/svg+xml`, and tests `srcset` only at the start of the value.
* `quench-ui/src/view.rs` `Element::style` compares property names case-sensitively and does not validate values; the property-name filter runs only at mount (`mount.rs`, `build_element`).
* `quench-ui/src/view.rs` `set_attr` lower-cases attribute names, so `viewBox` on SVG elements becomes `viewbox` (HTML parsers treat it case-insensitively for HTML, but not for SVG in the DOM via `setAttribute` on SVG elements). *Unverified:* whether the web backend compensates; `element("svg")` content is otherwise unexercised by tests.
* `quench-ui/Cargo.toml`: workspace dependency `quench-ui` has `version = "0.1.0"` while the crate is 0.1.3.
* `quench-ui/src/lib.rs:36` the root exports `Handler` etc.; `NodeId` is not exported at the root although `TestRenderer::root()` returns it (users must import `test_renderer::NodeId`).
* `quench-ui/src/test_renderer.rs` `Event::Resize` doc says it is "reported once when first laid out"; no core code emits it, backend-dependent. *Unverified.*
# Gap report: quench-ui router, form, i18n, kit, platform, anim, gesture

Scope: `quench-ui/src/{router,form,i18n,kit,platform,anim,gesture}.rs`; pages `usage-app.md`, `api-router.md`, `api-form.md`, `api-i18n.md`, `api-kit.md`, `api-platform.md` in `docs/crates/quench-ui/`.

## 1. Stale or wrong statements in the old docs (`docs/quench-ui.md`)

* Router: `Router::new(Rc::new(BrowserHistory::new()))` is shown as if `BrowserHistory` were part of `quench_ui::router`; it is in `quench-ui-web` (`quench-ui-web/src/history.rs`).
* Component kit: "Touch targets are at least 48 px" is a statement about CSS in `quench-style`, not about anything in `kit.rs`; nothing in this crate sets sizes.
* i18n: "Plural rules are CLDR's for English and Polish ... French and Portuguese": French/Portuguese lack CLDR's `many` category (large round numbers) and the code treats any `i == 0` (even 0.5) as `one`; every other language, including Russian, Czech and Arabic, silently uses the English rule, which is not their CLDR rule.
* i18n: "a language that lacks a message falls back to the first one given" is correct, but the old text omits that the plural rules then come from the fallback catalog's name.
* Form: "the host does not write the same value back into the input" is a backend property (`quench-ui-web`), not visible in `form.rs`; only the composing guard is in this crate.
* `kit::link`/router doc comment (in the source, not the old doc): `link` says "A relative `href` is followed without a page load"; the code follows hrefs that start with a single `/` (app-absolute), and treats everything else as an ordinary anchor.

## 2. Undocumented before (now documented)

* Exact grammar of the Fluent subset: entry rules, continuation lines (indentation trimmed per line), term/attribute dropping, selector choice order (exact number, plural category, default; text keys exact), depth limit 8, literals without escapes, `NUMBER` options ignored, all `ParseError` messages and that `line` is where the message starts.
* `plural_category` input handling: judged by written form, sign ignored, `u64` parse failure counts as 0; `Zero`/`Two` are never returned.
* Language lookup in `I18n::tr` (`pl-PL` finds `pl`, not the reverse; plural rules use the catalog's name).
* `Field::set` clears (untouched) or re-validates (touched) the error; only `blur` sets `touched`; `Form::validate` checks all fields; `apply_errors` exact rules for the form-level message; `Form::element` ignores submits while `submitting`.
* Markup emitted by every kit component (roles, classes, ids, aria attributes), `presence` state machine and constants (20 ms enter delay, +50 ms leave slack), `Recognizer` velocity and swipe tests, `Config` defaults, `Axis` classes (`q-touch-none`, `q-touch-pan-y`, `q-touch-pan-x`).
* `platform::after` runs the function immediately without a platform (also affects `kit::toast` and `presence`); `Timer` drop does not cancel; `ManualPlatform` semantics.
* `Router::navigate` replaces the entry for an identical location; `MemoryHistory::push` drops forward entries; redirect handling (8 hops, replace semantics, redirect to a non-matching URL).

## 3. Open questions and suspected bugs

* `i18n.rs` `Catalog::parse` (around line 190, `parse_pattern(value.trim_start_matches(' '), ...)`): a value that begins on the line after `=` keeps a leading `\n` (`a =\n    value` gives `"\nvalue"`); real Fluent drops it. Verified with a local run.
* `i18n.rs` `entry`: a continuation line that looks like an attribute (`    .x = y`) outside braces ends the message and is dropped, even when it is meant as text.
* `i18n.rs` `Catalog::parse`: `ParseError.line` is the start line of the message, not the line of the fault in multi-line messages.
* `i18n.rs` `plural_category`: `fr` treats `0.5` as `one` (integer part 0); CLDR `fr` agrees (`i = 0,1`), but `pt` in CLDR is `i = 0..1` too, while Portuguese `many` is missing. Languages like `ru`, `cs`, `ar` fall back to English silently.
* `kit.rs` `text_field`: the hint `<p id="<name>-hint">` is never referenced by `aria-describedby` (only the error id is), so a screen reader does not read the hint; also `hint` doubles as `placeholder`.
* `kit.rs` `toast`: the clearing timer is not cancelled by a later `toast` with the same text, so the older timer can clear the newer message early; also with no platform installed the message is cleared at once.
* `kit.rs` `pull_to_refresh`: `aria-label="Refreshing"` is a hard-coded English string (not localizable); `bottom_nav`'s `NavItem` glyph and labels are caller-provided but the progressbar label is not.
* `kit.rs` `tabs`: `aria-controls` of every tab points to the single `<id>-panel`; with `selected` out of range `panel(selected)` is still called. With an empty `labels` slice the `End` key handler would compute `count - 1` (underflow), but no tab exists to receive it.
* `kit.rs` `sheet`/`dialog`: `aria-label` duplicates the `h1` text instead of `aria-labelledby`; the modal `Escape` handler only runs while focus is inside the dialog.
* `router.rs`: `ChildRoute.guard` for nested layouts is set but unused (`add_layout` uses the inner layout's own guard); harmless duplicate.
* `router.rs` `Router::new` / `navigate`: `navigate` pushes to the history before guards run; a guarded route that redirects therefore replaces the pushed entry, so `back` skips it (intended, by `replace`). A `Deny` leaves the denied URL in the history.
* `router.rs` `link`: `aria-current` compares raw (non-decoded) paths, so `/a%20b` and `/a b` differ.
* `gesture.rs` `on_gesture`: pointer events of several pointers feed one recognizer (no multi-touch tracking); no `pointer id` check.
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
# Gap report: quench-ui-web and quench-ui-web-macros

## 1. Stale or wrong statements in the old docs (`quench-ui-web.md`, `quench-ui-web-macros.md`)

* Old page: "`app!` ... installs the panic hook, then builds the view". The code (`src/lib.rs` `run`) also installs `WebPlatform` through `quench_ui::platform::install`; not mentioned.
* Old page: "`quench.js` (under 400 lines)". It is 399 lines: correct but fragile.
* Old page: "`q_log` level 3 also shows an overlay on localhost, 127.0.0.1, [::1] or when `window.QUENCH_DEV` is true". Incomplete: `dev` can also be forced by `start(url, { dev })` and `?dev=1|0` on the script URL, and those take precedence (`js/quench.js` top of `start`, last lines).
* Old page: "`q_prevent` ... Called after `q_listen`" is true of the Rust call order but JS does not require it (the mode is looked up when the event fires).
* Old page said bridge answers "from any thread"; that is a statement about the host, not verifiable in this repo.
* Old page: the Rust API table omits `WebPlatform`, `BrowserHistory`, the `browser` module (`time_zone`, `wall_ms`, `storage_get/set`), `kind_code`/`decode_event`, `pending_tasks`/`pending_fetches`, `Sleep`, `AbortHandle`, the whole `plugins` module (11 clients, 22 data types) and `mock`'s API.
* Old page says `Request::header` drops names/values with a line break; the code also drops names containing `:` (`src/fetch.rs` `header`).
* Old macros page: "`Echo::NAME == "echo"`" is correct; "documentation and visibility are kept" is correct but it does not say the trait's attributes go on the struct and that a `#[derive(Clone, Copy, Debug)]` is added.
* Old pages and source comments refer to `docs/quench-ui-web.md`, now deleted: `quench-ui-web/src/lib.rs:4`, `src/host/wasm.rs:4`, `js/quench.js:2` and `docs/quench-android.md`/`quench-desktop.md` (links fixed). The source comments were not changed (docs-only rule); they should point at `docs/crates/quench-ui-web/internals.md`.
* `plugins.rs:4` says "what each method does is in `docs/quench-android.md`"; that page still exists.
* `docs/README.md` file tree and the root `README.md` index linked the deleted flat pages; links repointed.

## 2. Undocumented before, now documented

* The exact 30 imports and 11+ exports with argument lists, buffer ownership rules (`q_alloc` zero length returns 0; unknown address reads as empty data; `take` truncation).
* Boot order of `q_set_location`, `q_set_state`, `q_set_env`, `q_main?.()` (optional call) and the `instantiateStreaming` fallback.
* `q_history` treats every `op` other than 0, 1, 2 as forward (`js/quench.js` `go`).
* `Sleep` starts its timer at first poll; `WebPlatform::cancel` does not clear the browser timer (the sleeping task runs to the end).
* `EventSource::next` delivers `Closed` once, then `None`; `Request` GET/HEAD bodies are never sent.
* `BrowserHistory::back/forward` always return `true` and do not update the stored location until `popstate`; subscribers are never removed.
* `bridge::on_event`: single pump task, never restarted, events before the source opens are lost; `bridge::reset`, `browser::reset` are public on native targets.
* Bridge protocol details: 3 s connect wait, trusted-message check, only one bridge event source at a time (`bridgeEvents` holds the last opened id), desktop `webkit.messageHandlers.quench` path.
* Whole `quench_ui_web::mock` API (`Call`, `FetchCall`, 31 helpers) and what it does not simulate.
* The `#[plugin]` compile errors, return type rule, and that trait generics/supertraits/where clauses are silently dropped.

## 3. Open questions and suspected bugs

* `quench-ui-web-macros/src/expand.rs` (`method`, `keys` built from `Ident::to_string`): for a raw identifier parameter (`r#type`) the JSON key would be `r#type` including the prefix; `plugins::Incoming` has `r#type` as a field, so the same question applies to quench-serde derive. Not tested either way.
* `expand.rs` `plugin`: generics, supertraits, `where` clauses and `unsafe`/`auto` markers on the trait are ignored without error.
* `js/quench.js` `bridgeCall`: `JSON.parse(message)` of an invalid envelope throws inside `guarded`; the error is only a `console.warn` and the Rust fetch never completes (hangs forever). Rust always sends valid JSON, so it is theoretical.
* `js/quench.js` `bridgeAnswer`: `JSON.parse(data)` of a host message that is not JSON throws inside the message handler (uncaught in the port callback).
* `js/quench.js` `q_sse_open` for the bridge: a second `EventSource::open("quench-bridge:events")` overwrites `bridgeEvents`; closing the first source then does nothing (the `if (bridgeEvents === id)` guard) but the second keeps working. Intentional? Only one is supported.
* `src/exec.rs` `WebPlatform::after` (`src/platform.rs`): cancelled timers keep a pending task until the browser timer fires (visible in `pending_tasks()`); cheap but a leak for long timers.
* `src/fetch.rs` ids are a wrapping `u32` counter with no collision check against pending ids (needs 4 billion requests; theoretical).
* `src/lib.rs` doc comment on `app!`/`run`: `app!` expands `unsafe(no_mangle)` inside the user's crate; a crate with `#![forbid(unsafe_code)]` cannot use it on wasm32. Possibly intentional.
* `src/ffi.rs` `q_dispatch_event` is declared with `#[allow(clippy::must_use_candidate)]` and returns u32 flag; fine. `src/ffi.rs` `take`: an unknown nonzero address with `len > 0` silently reads as empty data, which hides double-use bugs.
* Wasm build profile (`wasm`) and the CSP statements could not be verified from this crate; see *Unverified* lines in the pages.
* A previous `git rm --cached` on `docs/quench-ui-web.md` was run by mistake while deleting the file (staging only; the file is deleted in the working tree as intended). The index entry for that file is therefore staged-as-deleted rather than unstaged; no other git state was changed.
# Gap report: quench-css

## 1. Stale or wrong statements in the old docs (docs/quench-css.md, now deleted)

* "Not supported: ... `:is()`/`:where()`/`:has()`, container queries": `@container` is not rejected; its body is inlined unconditionally with the condition ignored (sheet.rs:270). `:is`/`:where` are parsed as `Unknown` and never match.
* "A value that does not parse leaves the style as it was": false for keyword properties (display, position, text-align, text-transform, box-sizing, font-style, flex-wrap, visibility, list-style), which map any unrecognised keyword to a value (style.rs:1398-1420, 1854-1878). `background-color: <invalid>` clears the background (style.rs:1563).
* "`SUPPORTED` and `IGNORED` ... a supported property whose value the engine refuses" fails the test: true only for properties without var()/env().
* "`background` ... gradients are not drawn": gradients are not parsed at all and the background becomes transparent unless another colour is present.
* The old page did not state that `inherit` works for only six properties, that `initial`/`unset` do nothing, or that unknown properties stay in the rule.
* Old text "`animation` ... seven longhands": correct; "`transition` four longhands": correct.

## 2. In the code but previously undocumented

* `@layer { }` bodies inlined; `@-webkit-keyframes`; `@supports` semantics (value not examined, no precedence).
* Media: range syntax, `any-hover`/`any-pointer`, `print`/`speech` never match, `em`/`rem` = 16px, 0.01 equality tolerance.
* Tokenizer details (unquoted url, no identifier escapes, exponents).
* Specificity encoding and caps; `:disabled`/`:checked` also via attributes.
* Rounding/zoom rules, font keyword sizes, `border` default width 3px, post-processing (zeroed widths, blockification).
* `style_with`, `accepts`, `TouchAction::and/allows`, `Stylesheet::ignored` formats.
* `Calc` folding; `clamp` as max/min.

## 3. Open questions / suspected bugs

* style.rs:529 `Cascade::add` offsets order by `rules.len()`, not the max order of earlier sheets. Reproduced: `a{} b{} p{color:red}` then `p{color:blue}` yields red.
* lib.rs:25 `AttrOp` is not re-exported but appears in `Simple::Attribute`.
* style.rs:1408 `display: contents|table|inherit|initial` become `block`; 1412 `position: foo` becomes `static`; same pattern at 1854, 1863, 1666, 1845, 1879.
* style.rs:1503 vs 1507: `padding` rejects negatives, `padding-top` etc. accept them.
* sheet.rs:270 `@container` condition ignored (rules always apply); `@layer` order ignored.
* sheet.rs:329 a statement with an identifier but no colon is dropped without an `ignored` entry; keyframe selectors that are invalid are dropped without an entry (sheet.rs:301).
* style.rs:548 `style_with` skips the font-size-first pass and post-processing.
* selector.rs:360 dead branch for `is`/`where` (both arms give `Unknown`).
* selector.rs `Complex::matches` indexes `parts.len() - 1`: panics on an empty `Complex` (only constructible by hand, fields are pub).
* token.rs: identifier escapes unsupported (class names such as `.sm\:flex` cannot be written).
* A failed `var()` leaves the previous/inherited value instead of behaving as `unset` (no computed-value-time invalidity).
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
