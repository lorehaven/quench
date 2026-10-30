---
title: Internals
parent: quench-style
nav_order: 5
---

## Output shape

`CssRule::render` writes real **CSS Nesting**: a child inside a parent is emitted inside it, and a child selector `&` or `&:hover` refers to the parent. The shared stylesheet relies on it (`&::-webkit-scrollbar`, `&.show`, `& { ... }` inside `@media`). Consumers must read nested CSS: current browsers, or `quench-css` (which implements nesting). Flatten before use in an engine that does not.

`Theme::theme(t)` is a plain `:root` block plus three helpers, no nesting.

## Theme output

For any theme: `:root { --bs-warning; --bs-success-500..900; --bs-danger; --bs-code-bg; --bs-gray-50..950 }` followed by `.color-green`, `.color-yellow`, `.color-red` (`color: var(--bs-success-700)`, `var(--bs-warning)`, `var(--bs-danger)`). The two `Default*` and two `Bootstrap*` palettes differ only in values.

## Shared stylesheet

`theme_shared()` is the concatenation (joined by `"\n"`) of the rules of `root`, `header`, `content`, `footer`, `elements`, `modal`. Top-level selectors, in order:

* root: `:root` (the `--q-shell-*` properties), `html, body` (`height: 100%`, margin and padding 0, `user-select: none`), `.app, .q-shell-app` (full-viewport column flex), `*` (font family and colour from the shell properties), `*` with nested `&::-webkit-scrollbar...` rules.
* layout: `.q-shell-header`, `.q-shell-header-left`, `.q-shell-nav-trigger`, `.q-shell-content`, `.q-shell-content-inner`, `.q-shell-footer`.
* elements: `h1`, `h2`, `h3`, `form` (with `.error`), `input`, `.password-wrapper`, `button`, `select`, `section`, `.separator`, `.separator-or`, `.loader`, `.tabs` (`.tab`, `.tab.active`), `.slides-container` (`.slide`, `.active`, `.inactive`), `.binding-code-input` (`.binding-code-digit`), `.table` (`.header`, `.body`, `.row`, `.empty`, `.cell`, `.cell.sortable`, `.indicator`, `.cell.actions`, `.cell.buttons`), `.body > .row`, `.table-mobile` (`.buttons-bar`, `button.action`), with `@media screen and (max-width: 768px)` blocks.
* modal: `.modal-overlay` (`.show`), `.q-shell-modal-side`, `.q-shell-modal-content`, `.modal-side`, `.modal-center` (with `.section-title`, `.section-subtitle`, `.filter-field`, `button.nav`, `a.nav-entry-active`, `.code`, `.center-text`, `.buttons`) and media blocks for `max-width: 768px` and `max-height: 1080px`.

The exact declarations are in `src/styles/common/*.rs`; the quickest way to see them is `println!("{}", quench_style::theme::theme_shared())`.

## Component classes

`components::stylesheet()` (all `q-` prefixed): `q-vlist`, `q-vlist-space`, `q-vlist-window`, `q-vlist-row`; `q-anim`, `q-anim-in`; `q-touch-none`, `q-touch-pan-y`, `q-touch-pan-x`; `q-appbar`, `q-iconbtn`, `q-bottomnav`, `q-bottomnav-item`, `q-fab`; `q-modal`, `q-scrim`, `q-sheet`, `q-dialog`, `q-modal-title`, `q-dialog-actions`; `q-snackbar`, `q-snackbar-action`; `q-switch`, `q-checkbox`, `q-textfield`, `q-field-error` (and `q-form-error`, `q-textfield-error`), `q-chip`; `q-tablist`, `q-tab`, `q-tabpanel`; `q-list`, `q-tile`, `q-tile-text`, `q-tile-subtitle`; `q-ptr`, `q-ptr-top`, `q-ptr-indicator`, `q-ptr-content`, `q-ptr-busy` (with `@keyframes q-spin`); `q-swipe`, `q-swipe-back`, `q-swipe-action`, `q-swipe-front`; and `@media (prefers-reduced-motion: reduce)` blocks. Colours use the theme's custom properties only, so one stylesheet serves all themes. The generated stylesheet is about 12 KB.

## Contrast guarantees

The tests (not the runtime) check that `TEXT_PAIRS` reach 4.5:1 and `EDGE_PAIRS` 3:1 in all four themes. The error-text colour is `mix:--bs-danger:--bs-gray-100:60` because the plain danger colour reads below 4.5:1 on dark surfaces.

## Fluent subset

`ftl::parse` reads the part of Fluent that translation bundles need:

* Comments: lines starting with `#`, `##` or `###` followed by a space (or alone).
* Messages: `id = text`, where an id starts with an ASCII letter and continues with letters, digits, `_`, `-`. Values continue on following indented lines (and on lines starting with `{` or an attribute); leading line breaks are dropped, later ones kept.
* Placeables: `{ $name }` becomes the token `{$name}` (whitespace inside trimmed). **Everything else in braces contributes nothing**: terms (`{ -brand }`), message references, functions, string literals and selector expressions (`{ $n -> [one] ... }`) all yield an empty string in their place (a message that is only a selector has an empty value).
* Terms (`-brand = X`) are parsed and skipped (not in the output). Attributes (`.title = T`) are parsed and skipped; a message that has only attributes is not listed.
* Anything else at the start of a line is an error (`expected a message, a term or a comment`). `\r\n` is normalised to `\n`.

Example (from running the parser):

```text
hello = Hello, { $name }!         ->  Message { id: "hello", value: "Hello, {$name}!" }
multi =                           ->  Message { id: "multi", value: "line one\nline two" }
    line one
    line two
sel = { $n ->                     ->  Message { id: "sel", value: "" }
  [one] x
 *[other] y
}
```
