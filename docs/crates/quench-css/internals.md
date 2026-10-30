---
title: Internals
parent: quench-css
nav_order: 12
---

Design notes that matter to users: how a style is computed, the order rules, and the quirks that follow from the implementation. Everything is from `src/sheet.rs`, `src/style.rs`, `src/selector.rs`.

## Parsing pipeline

`parse(text)`: `tokenize` -> one pass over the tokens (`Parser::block`). The parser has no separate grammar for the prelude: for each statement it finds the end (`;`, `{` or `}` at bracket depth 0), then:

* `AtKeyword` first: an at-rule. With `{`: handled by name or ignored (the block is skipped by bracket matching). Without: ignored.
* Otherwise, ending at `{`: a rule (selector list = the tokens before it). Otherwise a declaration.
* Stray `}` and `;` are skipped. An unclosed block ends at the end of input.

Values stay as `Vec<Value>` until the cascade needs them; nothing is typed at parse time except media queries and selectors. This is why an unsupported value is only noticed when a style is computed (the declaration is then skipped).

## Rule order

Every rule block (including empty ones and `@media`) takes the next number of a counter before its nested content is parsed, so a nested rule has a later order than its parent, and the parent's own declarations keep the parent's number even if they are written after the nested rule. A selector list with N selectors creates N `Rule`s with the same `order`. `Stylesheet::rules` is sorted by `order`.

## Order across sheets

`Cascade::add` starts each sheet's source order past the highest `order` added so far (a `next_order` counter), so a later sheet's rules win ties however many blocks of an earlier sheet produced no `Rule`.

## The compute pipeline

`Cascade::compute_with(element, parent, root_font_size, inline)`:

1. `parent` defaults to `ComputedStyle { font_size: 16.0 * zoom, ..Default }`. The new style is `parent.inherited()`.
2. For each rule whose `media` levels hold (`all_match`) and whose selector matches, each of its declarations becomes a candidate `(important, specificity, order, index, declaration)`. Inline declarations are candidates with `(important, Specificity::MAX, usize::MAX, index)`.
3. Candidates are sorted ascending by that tuple (a stable sort over a tuple key); later entries override earlier ones when applied.
4. Custom properties: `raw` is the parent's resolved map overlaid with the element's winning `--x` declarations in sorted order (last wins). If the element has any, each is substituted (`var()` resolved against `raw`, recursion depth limit 16) in sorted order; a success is inserted into the style's map, a failure removes the name. If the element declares none, the parent's `Rc` is shared.
5. Every other candidate is substituted against the style's resolved custom map; one that fails substitution (a `var()` without value or fallback, depth > 16) is dropped.
6. Font size first: on a scratch copy, `font` and `font-size` candidates are applied with `Units` whose `font_size` is the parent's. The result is the element's font size for `em` in everything else.
7. All candidates are applied in order. `font`/`font-size` use the parent's units again; the others use the element's. An `apply` that fails (value does not parse, unknown property) leaves the style unchanged.
8. Post-processing: border sides and outline with style `none` get width 0; absolutely positioned/fixed boxes are blockified.

The consequences: there is no "computed-value time invalid" behaviour (a failed `var()` does not reset the property to inherit/initial: the property keeps the inherited/initial value from step 1 unless an earlier candidate set it); and `inherit` keyword support is per property, not global.

### Specificity encoding

`(ids << 16) | (classes << 8) | tags` in a `u32`, each part saturating at 255 so that parts never carry into each other. `:not(...)` takes the maximum specificity among its arguments and adds its parts. Nested rules are resolved before specificity is computed, so `.a { &:hover {} }` is `.a:hover` (0,2,0).

### Inheritance

Implemented by `ComputedStyle::inherited`, a fixed list (see the [style field table](api-style.md#computedstyle)). There is no per-property registration; explicit `inherit` is honoured only by `color`, `font-family`, `font-size`, `font-weight`, `line-height` and `font`.

## `var()` and `env()` substitution

`substitute(values, custom, depth)` walks the component values recursively, including inside functions and blocks:

* `var(--x)`: the first argument must be an identifier; found in `custom` -> its value (itself substituted, `depth + 1`); not found -> the fallback (arguments after the first comma, with commas) substituted; neither -> `None`.
* `env(name)`: described on [Supported](supported.md#functions). Result is a `Dimension(px)`.
* Depth above 16 returns `None` at any level (this is also how cycles end).

## `calc()` evaluation

`parse_length` builds a `Calc` tree during computation, with `em`/`rem`/viewport units already converted to pixels. Only percentages remain symbolic, so `Calc` has no unit-kinds beyond `Px` and `Percent`. After parsing, the tree is folded: any tree without a percentage becomes `Length::Px`, a lone percentage `Length::Percent`, otherwise `Length::Calc(Rc<Calc>)`. `Length::resolve(basis)` evaluates later (layout supplies `basis`). `min`/`max` of constants fold; `clamp(a, b, c)` is stored as `max(a, min(b, c))`, so when `a > c` the lower bound `a` wins, as in CSS.

## Nesting

`resolve_nesting` is purely structural (see [API](api-selector.md#resolve_nesting)). Because `:not()` arguments and attribute selectors are untouched, `&` inside `:not(&)` is not resolved (and `has_nesting` does not look inside `:not`); such a selector is kept with an unresolved `Nesting` condition, which never matches.

## Media evaluation

Queries are parsed once into `MediaQuery` values stored on each `Rule`; evaluation happens per `compute` call with the cascade's current `MediaEnv`. Changing `Cascade::media` changes results without re-parsing. `@supports` is the opposite: evaluated once, at parse time, against the static property lists.

## Error recovery summary

| Situation | Result |
|---|---|
| Bad selector in a list | That selector dropped, `selector in ...` logged. |
| Rule with no valid selector | Dropped with its nested rules. |
| Declaration with unknown property | Kept, ignored at apply time, logged. |
| Declaration with bad value | Kept; skipped at compute time (earlier value stays). |
| Unknown at-rule | Skipped with its block; logged. |
| Unbalanced brackets | The block extends to the end of input. |

## Performance traits

`compute` iterates over **all** rules and tests `all_match` and `selector.matches` for each: O(rules x selector depth) per element, no indexing by tag/class/id. Each `Cascade::add` clones the sheet's rules (the declaration lists are shared `Rc`s). `Element` handles are cloned during matching, so keep them cheap. Strings are allocated for tag/class/attribute queries (`Element::tag()` returns `String`).

## Known quirks (also in the gap report)

* [Sheet order offset](#order-across-sheets).
* Keyword fallbacks that accept garbage ([Supported](supported.md#value-fallbacks)).
* `@container` bodies apply unconditionally.
* `AttrOp` is not exported.
* `background-color: <invalid>` clears the background.
* `padding` rejects negative values but `padding-top` etc. do not.
* `style_with` does not re-run the font-size-first pass or the post-processing.
