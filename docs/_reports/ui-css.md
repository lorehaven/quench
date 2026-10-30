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
