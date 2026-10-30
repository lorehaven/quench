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
