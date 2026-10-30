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
