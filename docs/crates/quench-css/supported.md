---
title: Supported CSS
parent: quench-css
nav_order: 11
---

The exact subset of CSS the engine reads, derived from `src/*.rs` of `quench-css` 0.1.2. "Ignored" means the construct is parsed or skipped without effect; "never matches" means a selector is kept but cannot match any element. Where this page says "any other keyword", the property does **not** reject unknown keywords; see [Value fallbacks](#value-fallbacks). Layout-side behaviour of the resulting fields is on the [quench-layout supported page](../quench-layout/supported.md).

* [Syntax](#syntax)
* [Selectors](#selectors)
* [At-rules](#at-rules)
* [Properties](#properties) ([property list](#property-list), [shorthands](#shorthands), [ignored properties](#ignored-properties), [not recognised](#not-recognised))
* [Units](#units), [colours](#colours), [functions](#functions), [custom properties](#custom-properties)
* [Media queries](#media-queries)
* [Cascade](#cascade)

## Syntax

| Construct | Behaviour |
|---|---|
| Comments `/* */` | Dropped. Unterminated: runs to the end. |
| Strings `"..."`, `'...'` | Backslash makes the next char literal; hex escapes not decoded. |
| Identifier escapes (`\:`) | Not supported (see [token](api-token.md#tokenize)). |
| `!important` | Recognised at the end of a declaration, any case, whitespace allowed (`! important`). |
| Numbers | Sign, fraction, exponent; units are lowercased (`10PX` is `px`). |
| Unquoted `url(...)` | One raw string up to the first `)`. |
| Error recovery | A statement ends at the first `;`, `{` or `}` outside brackets. A bad declaration/rule is skipped; nothing is fatal. |
| Unknown property | Declaration kept, no effect, logged as `property <name>` in `Stylesheet::ignored`. |
| Declarations outside any rule | Discarded. |
| Declaration without `:` | Dropped silently. |
| `<!--`, `-->`, `@charset` | Not recognised / ignored. |
| Nesting | Supported, see below. |

## Selectors

| Kind | Syntax | Support |
|---|---|---|
| Type | `div` | Matches `Element::tag()` case-insensitively. |
| Universal | `*` | Yes; only as the first item of a compound; counts no specificity. |
| Class | `.a` | `Element::has_class`; the class name must be an identifier (no escapes). |
| Id | `#a` | `Element::id`, exact. |
| Attribute exists | `[a]` | Yes. Attribute name lowercased. |
| `=` | `[a=v]`, `[a="v"]`, `[a=1]` | Equal. Value: identifier, string or number (numbers are compared as text of the number in `{value}{unit}` form, e.g. `1.5px`; `[a=1]` becomes `"1"`). |
| `~=` | | Whitespace-separated word. |
| `^=` | | Prefix; empty value never matches. |
| `$=` | | Suffix; empty value never matches. |
| `*=` | | Substring; empty value never matches. |
| `\|=` | | Equal, or starts with value followed by `-`. |
| Case flag | `[a=v i]` | `i` (case-insensitive, ASCII). `s` is not recognised (treated as no flag). |
| Descendant | `a b` | Yes. |
| Child | `a > b` | Yes. |
| Next sibling | `a + b` | Yes (via `Element::previous_sibling`). |
| Later sibling | `a ~ b` | Yes. |
| Selector list | `a, b` | Yes; each selector separately; an unparseable one is dropped, the others kept. |
| Nesting selector | `&` | In nested rules only, see [Nesting](#nesting). |
| `:root` | | `Element::is_root`. |
| `:first-child` | | No previous sibling. |
| `:hover` `:focus` `:focus-visible` `:active` | | `Element::state`. |
| `:disabled` | | `state(Disabled)` or a `disabled` attribute present. |
| `:enabled` | | Not `:disabled`. |
| `:checked` | | `state(Checked)` or a `checked` attribute present. |
| `:not(list)` | | Yes, with a selector list (complex selectors allowed). Specificity of its most specific argument. |
| `:is()`, `:where()`, `:has()` | | Parsed as `Unknown`; the compound never matches. |
| Other pseudo-classes | `:last-child`, `:nth-child()`, `:only-child`, `:empty`, `:focus-within`, `:visited`, `:lang()`, ... | Never match (single-colon names become `Pseudo(name)` and count as a class in specificity). |
| Pseudo-elements | `::before`, `::after`, `::placeholder`, `::selection`, ... | Never match, no specificity. `:before` (one colon) is a `Pseudo("before")`: never matches, counts as a class. |
| Namespaces `a|b`, column `||` | | Not supported (selector dropped). |
| Whitespace/case | | Tag names are lowercased; class/id/attribute values keep their case. |

### Nesting

Handled while parsing (CSS Nesting, subset). For each parent selector `P` and nested selector `N`:

| Nested form | Result |
|---|---|
| `&.on`, `&:hover`, `&[x]`, `&#id` (leading `&`) | The conditions of `N`'s first compound are merged into `P`'s last compound. |
| `a&` (tag with `&`) | Merged; if `P`'s last compound has another tag, the selector is dropped (`nesting in ...` logged). |
| `&.on .x`, `& > .x` | The first compound merges into `P`'s last compound; the remaining compounds are appended unchanged. |
| `&` in a later compound (`.a &`) | `&` is only handled in the **first** compound; the selector is dropped and `nesting in ...` is logged. |
| `.x` (no `&`) | Descendant of `P`: `P .x`. |
| `> .x`, `+ .x`, `~ .x` | Keeps the leading combinator relative to `P`. |
| `@media` inside a rule | The rule's declarations in that block apply under the media level. |
| `@supports` inside a rule | Evaluated now; true inlines the declarations/rules into the rule. |
| `&` at top level | Dropped, logged ``` `&` outside a rule in ... ```. |
| Parent list × nested list | Cross product. |

Declarations of a rule written after a nested rule still belong to the rule (they are not a separate "anonymous" rule as in the full spec). Nested rules get later source order than their parent.

## At-rules

| At-rule | Support |
|---|---|
| `@media` | Yes; kept on each rule (levels accumulate when nested), evaluated against `MediaEnv` at compute time. See [Media queries](#media-queries). Without a `{}` block: ignored. |
| `@supports` | Evaluated at parse time. `(prop: value)` is true iff the property is known (`SUPPORTED`, `IGNORED` or `--custom`); the value is not examined. `not`, `and`, `or` (left to right, no precedence), nested parentheses. `selector()` and other functions: false. A true condition inlines the body; false drops it. |
| `@keyframes`, `@-webkit-keyframes` | Stored in `Stylesheet::keyframes` (name: identifier or string; frames `from`, `to`, `N%`, lists). Both spellings share one name space. |
| `@layer name { ... }` | The body is inlined; the layer name and layer ordering have no effect. |
| `@layer a, b;` | Ignored (logged `@layer`). |
| `@container (...) { ... }` | **The condition is not evaluated: the body is inlined unconditionally.** |
| `@import`, `@charset`, `@namespace`, `@font-face`, `@page`, `@property`, `@counter-style`, `@font-feature-values`, `@scope`, `@starting-style`, any other | Ignored with their block, logged as `@<name>`. |

## Properties

### Value fallbacks

Because a declaration is applied by matching on the value, several keyword properties map **any unrecognised single keyword** to one of their values instead of rejecting the declaration (as a browser would). They still require exactly one identifier (otherwise the declaration is dropped). This includes `inherit`, `initial`, `unset`, `revert`.

| Property | Unrecognised single identifier gives |
|---|---|
| `display` | `block` (so `contents`, `flow-root`, `table`, `inherit`, `initial` are `block`) |
| `position` | `static` |
| `box-sizing` | content-box (`border_box = false`) |
| `flex-wrap` | wrap (`true`) for any identifier other than `nowrap` (including `wrap-reverse`) |
| `font-style` | normal (`italic = false`) for any identifier other than `italic`, `oblique` |
| `text-align` | `start` (so `justify`, `inherit`, `initial` are `start`; `match-parent` too) |
| `text-transform` | `none` (so `full-width` and `inherit` are `none`) |
| `list-style` | `disc` unless a component is `none` or `decimal` (any value, any number of components) |
| `visibility` | visible unless `hidden` or `collapse` |
| `background` / `background-color` | transparent, when no component parses as a colour (so an invalid colour *clears* the background) |
| `background-image` | none, when there is no `url()` component (gradients, `none` and garbage all clear the image) |
| `font-family` | an empty list is accepted |

Global keywords: `inherit` is honoured only for `color`, `font-family`, `font-size`, `font-weight`, `line-height` and `font` (copies the parent's value). `initial`, `unset`, `revert`, `revert-layer` are not implemented anywhere (`flex: initial` is the only use of the word). A declaration with an invalid value never changes the style (the earlier value stays).

### Property list

`Initial` is the `ComputedStyle::default()` value. `Inh.` is whether the child starts from the parent's value (`ComputedStyle::inherited`). A "length" accepts the forms of [Units](#units) and [Functions](#functions): `px`-family dimensions, percentages, `0`, and `calc()`/`min()`/`max()`/`clamp()`. "Exactly one" means a declaration with more or fewer components is dropped. Length pixels are multiplied by `zoom` for absolute units.

#### Box and position

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `display` | one keyword: `none`, `inline`, `inline-block`, `flex`, `inline-flex`, `grid`, `inline-grid`, `list-item`; any other keyword gives `block` | `inline` | no |
| `position` | one keyword: `relative` and `sticky` (both `relative`), `absolute`, `fixed`; any other `static` | `static` | no |
| `top`, `right`, `bottom`, `left` | exactly one: `auto` or a length | auto | no |
| `inset` | 1 to 4 of `auto` / length (top, right, bottom, left order) | all auto | no |
| `z-index` | exactly one: `auto` or a bare number (rounded to `i32`) | auto | no |
| `width`, `height`, `min-width`, `min-height` | exactly one: `auto` or a length (`min-content`, `max-content`, `fit-content`, `stretch` are not accepted) | auto | no |
| `max-width`, `max-height` | exactly one: `none` or a length (`auto` is not accepted) | none | no |
| `box-sizing` | one keyword; `border-box` sets border-box, any other content-box | content-box | no |
| `margin` | 1 to 4 of `auto` / length (negative allowed) | 0 | no |
| `margin-top`, `-right`, `-bottom`, `-left` | exactly one: `auto` or length | 0 | no |
| `padding` | 1 to 4 lengths; a negative pixel value (or a negative constant `calc`) rejects the declaration; `auto` is rejected | 0 | no |
| `padding-top`, `-right`, `-bottom`, `-left` | exactly one length (negative accepted) | 0 | no |
| `overflow` | 1 or 2 of `visible`, `hidden`, `clip` (hidden), `scroll`, `auto`; two values are (x, y) | visible | no |
| `overflow-x`, `overflow-y` | exactly one of the same keywords | visible | no |
| `visibility` | one keyword; `hidden`, `collapse` hide; anything else visible | visible | yes |
| `opacity` | exactly one: number or percentage, clamped to 0..1 | 1 | no |
| `touch-action` | `auto` or `manipulation` (all), `none`, or a list of `pan-x`, `pan-y`, `pan-left`, `pan-right`, `pan-up`, `pan-down` (and ignored `pinch-zoom`); anything else rejects. A list of only `pinch-zoom` rejects | all directions | no |

#### Borders, outline, background, shadow

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `border` | any order: a width (`thin` 1px, `medium` 3px, `thick` 5px, or a constant non-negative length), a style, a colour; any other component rejects the whole declaration; sets all four sides; an omitted width is **3px**, style `none`, colour `currentcolor` | no border | no |
| `border-top`, `-right`, `-bottom`, `-left` | as `border`, one side | | no |
| `border-width` | 1 to 4 widths | 0 | no |
| `border-style` | 1 to 4 of `none`, `hidden` (none), `solid`, `double`, `groove`, `ridge`, `inset`, `outset` (all solid), `dashed`, `dotted` | none | no |
| `border-color` | 1 to 4 colours (`currentcolor` allowed) | currentcolor | no |
| `border-<side>-width` / `-style` / `-color` | exactly one value of the kinds above | | no |
| `border-radius` | 1 to 4 lengths (tl, tr, br, bl) before an optional `/`; the part after `/` is discarded; percentages allowed | 0 | no |
| `outline` | as `border` (omitted width 3px, style none, colour currentcolor) | none | no |
| `outline-width`, `outline-style`, `outline-color` | exactly one width / style / colour | width 3px (style none) | no |
| `outline-offset` | exactly one constant length (no `%`), negative allowed | 0 | no |
| `background`, `background-color` | `none` (transparent); otherwise the **last** component that parses as a colour (including `currentcolor`) or transparent if none. `background` additionally resets the image layer and reads a `url(...)`, `no-repeat`, `repeat`, `cover`, `contain`, `center`; other components (gradients, positions, `fixed`, `padding-box`, ...) are skipped | transparent | no |
| `background-image` | the first `url("...")` component; anything else clears the image | none | no |
| `background-size` | one of `auto`, `cover`, `contain` (lengths rejected) | auto | no |
| `background-repeat` | one of `repeat`, `no-repeat` (`repeat-x`, `space`, ... rejected) | repeat | no |
| `background-position` | keywords `left`, `right`, `top`, `bottom`, `center` and percentages; the first percentage is horizontal, the second vertical; lengths and numbers reject. Missing axis is `50%` | `0% 0%` | no |
| `box-shadow` | `none`, or a comma list; each layer: optional `inset`, optional colour (default `currentcolor`), 2 to 4 constant lengths (x, y, blur, spread; no `%`); blur is clamped at 0 | none | no |

After the cascade, a border side whose style is `none` has width 0, and `outline-width` is 0 when `outline-style` is `none`.

#### Flexbox

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `flex` | `none` (0 0 auto), `auto` (1 1 auto), `initial` (0 1 auto); otherwise up to two non-negative numbers (grow, shrink; defaults 1, 1) and an optional basis (`auto` or length). Without a basis the basis is `0` (`flex: 1` is `1 1 0`). A third bare non-zero number rejects | grow 0, shrink 1, basis auto | no |
| `flex-direction` | `row`, `row-reverse`, `column`, `column-reverse` (others reject) | row | no |
| `flex-wrap` | one keyword; `nowrap` is no wrap, any other wrap | nowrap | no |
| `flex-grow`, `flex-shrink` | exactly one bare number, negatives clamped to 0 | 0, 1 | no |
| `flex-basis` | exactly one: `auto` or length (`content` rejected) | auto | no |
| `align-items` | `stretch`/`normal`, `flex-start`/`start`/`self-start`/`left`, `flex-end`/`end`/`self-end`/`right`, `center`, `baseline` | stretch | no |
| `align-self` | `auto` (inherit parent's align-items) or the `align-items` values | auto | no |
| `justify-content` | `flex-start`/`start`/`left`/`normal`, `flex-end`/`end`/`right`, `center`, `space-between`, `space-around`, `space-evenly` | start | no |
| `gap`, `grid-gap` | 1 or 2 lengths (row gap, column gap) | 0 | no |
| `row-gap`, `grid-row-gap`, `column-gap`, `grid-column-gap` | exactly one length | 0 | no |

#### Grid

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `grid-template-columns`, `grid-template-rows` | `none`, or a sequence of tracks: a length, `<n>fr`, `auto`/`min-content`/`max-content` (all `auto`), `minmax(a, b)` (each a track), `repeat(n, tracks)` (`n` a number >= 1, truncated, at most 1000 repeats), `repeat(auto-fill \| auto-fit, tracks)`; `[line names]` are skipped (also inside `repeat`) | no tracks | no |
| `grid-template-areas` | `none`, or one or more strings, each a whitespace-separated list of cell names (`.` is an empty cell); every row must have the same non-zero number of cells. Rectangularity of names is not checked | none | no |
| `grid-area` | one identifier other than `auto` (an area name); or 1 to 4 `/`-separated lines `row-start / column-start / row-end / column-end` (each a line, see below); a missing part is `auto`; the numeric form clears the area name | auto | no |
| `grid-auto-columns`, `grid-auto-rows` | exactly one track | auto | no |
| `grid-column`, `grid-row` | one or two `/`-separated lines | auto / auto | no |
| `grid-column-start`, `-end`, `grid-row-start`, `-end` | one line | auto | no |

A grid line is `auto`, a non-zero number (a line; negative counts from the end), `span N` or `N span` (`N` >= 1). Named lines are not supported. `grid-auto-flow`, `grid-template` and `place-*` are [not recognised](#not-recognised).

#### Transitions and animations

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `transition` | `none` or a comma list; each entry any order: times (`s`/`ms`/`0`; first is duration, second delay), a timing function, a property name (identifier, lowercased, `all` default). Any other component rejects the declaration | no transitions | no |
| `transition-property` | comma list of identifiers | | no |
| `transition-duration`, `transition-delay` | comma list of single times | | no |
| `transition-timing-function` | comma list of timing functions | | no |
| `animation` | `none` or a comma list; each entry any order: times (first duration, second delay), timing function, iteration count (number >= 0 or `infinite`), direction, fill-mode, `running`/`paused` (ignored), name (the remaining identifier) | no animations | no |
| `animation-name` | comma list; `none` gives an empty name | | no |
| `animation-duration`, `-delay` | comma list of times | | no |
| `animation-timing-function` | comma list of timing functions | | no |
| `animation-iteration-count` | comma list of numbers >= 0 / `infinite` | | no |
| `animation-direction` | `normal`, `reverse`, `alternate`, `alternate-reverse` | | no |
| `animation-fill-mode` | `none`, `forwards`, `backwards`, `both` | | no |

Timing functions: `linear`, `step-start` and `step-end` and `steps(...)` (all `Linear`), `ease`, `ease-in`, `ease-out`, `ease-in-out`, `cubic-bezier(...)` (read as `Ease`). Times are seconds; a negative duration becomes 0, a negative delay is kept. In `transition` a bare `0` is a time and any other bare number rejects the declaration; in `animation` every bare number (including `0`) is the iteration count. A transition longhand extends the transition list to the longhand's number of entries with `all 0s ease` entries, and cycles shorter lists; the animation longhands extend/cycle likewise. A longhand never shortens the list.

#### Text and fonts

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `color` | exactly one colour, or `inherit`; `currentcolor` resolves to the parent's colour | black | yes |
| `font` | `inherit`, or `[italic\|oblique\|normal\|<weight>]* <size>[/<line-height>] <family>`; the size is required; families as in `font-family` | see longhands | yes |
| `font-family` | comma list of strings and space-joined identifier runs; `inherit` | empty list | yes |
| `font-size` | `xx-small` 9px, `x-small` 10, `small` 13, `medium` 16, `large` 18, `x-large` 24, `xx-large` 32 (times zoom), `larger` (parent x 1.2), `smaller` (parent / 1.2), a percentage of the parent, a constant non-negative length (`em` is the parent's size); `inherit` | 16px | yes |
| `font-weight` | a bare number 1..=1000 (rounded), `normal` 400, `bold` 700, `bolder`, `lighter` (relative to the parent: bolder is 400 below 400, 700 below 600, else 900; lighter is 100 below 600, 400 below 800, else 700); `inherit` | 400 | yes |
| `font-style` | one keyword; `italic`, `oblique` give italic | normal | yes |
| `line-height` | `normal`, a bare number (multiple), a percentage (as multiple), a constant length (pixels); `inherit` | normal | yes |
| `text-align` | one keyword: `left`, `right`, `center`, `end`; any other `start` | start | yes |
| `text-transform` | one keyword: `uppercase`, `lowercase`, `capitalize`; any other `none` | none | yes |
| `list-style` | any components; `none` -> none, `decimal` -> decimal, otherwise disc | none (disc once the property is written) | yes |

#### Transform

| Property | Accepted values | Initial | Inh. |
|---|---|---|---|
| `transform` | `none`, or a space-separated list of `translate(x[, y])`, `translateX(x)`, `translateY(y)`, `scale(x[, y])`, `scaleX(n)`, `scaleY(n)`, `rotate(angle)`. Other functions (`skew`, `matrix`, `rotateX`, `translate3d`, `translateZ`, ...) reject the declaration. Replaces the previous transform entirely (including a `rotate` property) | identity | no |
| `rotate` | `none` or one angle; sets only the rotation of the folded transform (translate and scale are kept) | 0 | no |

Fold rules: translations add (two constant lengths add; two percentages add; any other mix keeps only the later one); scales multiply; angles add. Angle units: `deg`, `rad`, `turn`, `grad`, bare `0`. A `translate` y defaults to 0, `scale` y defaults to x. Order of functions is not preserved.

### Shorthands

| Shorthand | Expands to | Notes |
|---|---|---|
| `margin`, `padding`, `inset`, `border-width`, `border-style`, `border-color`, `border-radius` | four sides/corners | 1: all; 2: vertical, horizontal; 3: top, horizontal, bottom; 4: top, right, bottom, left. Corners: tl, tr, br, bl. More than 4 or any invalid component rejects. |
| `border`, `border-<side>` | width, style, colour | Unspecified parts reset to 3px / none / currentcolor. |
| `outline` | width, style, colour | Same. |
| `background` | colour, image, size, repeat, position | Resets the image layer even when only a colour is written. |
| `gap`, `grid-gap` | `row-gap`, `column-gap` | |
| `overflow` | `overflow-x`, `overflow-y` | |
| `flex` | grow, shrink, basis | |
| `font` | style, weight, size, line-height, family | Sets italic, weight, size, line-height and family together; resets the unwritten ones to normal/400/`normal`. |
| `transition`, `animation` | the lists | Replace the entire list. |
| `grid-column`, `grid-row`, `grid-area` | start/end lines | |
| `rotate`, `transform` | the folded `Transform` | See above. |
| `list-style` | one keyword | |

### Ignored properties

Accepted without effect and without validating the value (they have no handler; `accepts` returns `false`): `cursor`, `user-select`, `will-change`, `overscroll-behavior`, `overscroll-behavior-x`, `overscroll-behavior-y`, `overflow-anchor`, `scrollbar-width`, `scrollbar-color`, `accent-color`, `-webkit-tap-highlight-color`, `-webkit-touch-callout`, `-webkit-overflow-scrolling`. Custom properties (`--*`) are known and stored.

### Not recognised

Every property not in the two lists is logged as `property <name>` and has no effect. Among those people expect: `float`, `clear`, `order`, `align-content`, `justify-items`, `justify-self`, `place-items`, `place-content`, `place-self`, `flex-flow`, `grid-auto-flow`, `grid-template`, `grid`, `aspect-ratio`, `text-decoration`, `text-overflow`, `white-space`, `word-break`, `letter-spacing`, `word-spacing`, `text-indent`, `vertical-align`, `content`, `filter`, `backdrop-filter`, `clip-path`, `mask`, `object-fit`, `object-position`, `pointer-events`, `resize`, `transform-origin`, `border-image`, `border-collapse`, `columns`, `table-layout`, `font-variant`, `font-stretch`, `direction`, `writing-mode`, `counter-*`, `quotes`, `border-top-left-radius` and the other per-corner radii, `margin-inline*`/`padding-block*` logical properties, `inset-inline*`, `all`.

## Units

`parse_length` converts a dimension to pixels. `zoom` is `Cascade::zoom` (device pixels per CSS pixel).

| Unit | Pixels |
|---|---|
| `px` | `n * zoom` |
| `em` | `n * font_size` (the element's font size; for `font-size`/`font` the parent's) |
| `rem` | `n * root_font_size` (the argument of `compute`) |
| `ex`, `ch` | `n * font_size * 0.5` |
| `vw`, `svw`, `lvw`, `dvw` | `n * viewport_width / 100` |
| `vh`, `svh`, `lvh`, `dvh` | `n * viewport_height / 100` |
| `vmin`, `svmin`, `lvmin`, `dvmin` | `n * min(w, h) / 100` |
| `vmax`, `svmax`, `lvmax`, `dvmax` | `n * max(w, h) / 100` |
| `pt` | `n * 96 / 72 * zoom` |
| `pc` | `n * 16 * zoom` |
| `in` | `n * 96 * zoom` |
| `cm` | `n * 96 / 2.54 * zoom` |
| `mm` | `n * 96 / 25.4 * zoom` |
| `q` | `n * 96 / 101.6 * zoom` |
| `%` | kept as `Length::Percent`, resolved by layout against the property's basis |
| bare `0` | zero length |

Not supported: `lh`, `rlh`, `cap`, `ic`, `vi`, `vb`, container units (`cqw`, ...), `fr` outside grid tracks, a bare non-zero number as a length. Angles: `deg`, `rad`, `turn`, `grad`. Times: `s`, `ms`. `fr` is only read in track lists. Viewport units use `media.width * zoom`; media queries themselves use only `px`, `em`, `rem` ([below](#media-queries)).

## Colours

Accepted wherever a colour is read (`color`, `background*`, `border*-color`, `outline-color`, `box-shadow`).

| Syntax | Support |
|---|---|
| `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` | Yes (case-insensitive). |
| `rgb(r g b)`, `rgb(r, g, b)`, `rgba(...)` | Yes, 3 or 4 components, comma or space/slash separators (mixing them is accepted); channels numbers or percentages, alpha number or percentage. `rgb` and `rgba` are aliases. |
| `hsl(h s l)`, `hsl(h, s, l)`, `hsla(...)` | Yes; hue a number or `deg`; s and l percentages. `turn`/`rad`/`grad` hues reject. |
| `color-mix(in <space>, A [p%], B [p%])` | Yes. The first comma part must start with `in`; the space word is ignored and **all spaces mix as sRGB**, straight per channel including alpha; percentages: both given are normalised by their sum; one given, the other is `100 - p`; none: 50/50; sum <= 0 rejects. `currentcolor` inside a mix rejects. Interpolation hints (`longer hue`) are not read. |
| `transparent` | `rgba(0,0,0,0)`. |
| `currentcolor` | `ColorValue::CurrentColor` (case-insensitive). For `color` it resolves against the parent's colour. |
| Named colours | The 38 names below (case-insensitive). |
| `hwb()`, `lab()`, `lch()`, `oklab()`, `oklch()`, `color()`, `light-dark()`, system colours (`Canvas`, ...), relative colour syntax (`rgb(from ...)`) | Not supported. |

Named colours (r g b): `black` 0 0 0, `silver` 192 192 192, `gray`/`grey` 128 128 128, `white` 255 255 255, `maroon` 128 0 0, `red` 255 0 0, `purple` 128 0 128, `fuchsia`/`magenta` 255 0 255, `green` 0 128 0, `lime` 0 255 0, `olive` 128 128 0, `yellow` 255 255 0, `navy` 0 0 128, `blue` 0 0 255, `teal` 0 128 128, `aqua`/`cyan` 0 255 255, `orange` 255 165 0, `pink` 255 192 203, `brown` 165 42 42, `gold` 255 215 0, `indigo` 75 0 130, `violet` 238 130 238, `crimson` 220 20 60, `tomato` 255 99 71, `coral` 255 127 80, `salmon` 250 128 114, `khaki` 240 230 140, `lightgray`/`lightgrey` 211 211 211, `darkgray`/`darkgrey` 169 169 169, `dimgray` 105 105 105, `whitesmoke` 245 245 245, `rebeccapurple` 102 51 153, and `transparent`. Any other CSS named colour (`lightblue`, `darkblue`, `orangered`, `slategray`, ...) is not recognised.

## Functions

| Function | Where | Support |
|---|---|---|
| `calc()` | lengths | `+`, `-`, `*`, `/` with the usual precedence, parentheses, nested `calc`/`min`/`max`/`clamp`. `+`/`-` need both sides lengths or both numbers (`calc(1px + 2)` rejects); `*` needs one side a number; `/` divides by a non-zero number; `calc(2)` (a plain number) rejects. Operators need whitespace around `+` and `-`. Not allowed in colours, nor for number-typed properties (`opacity`, `z-index`, `flex-grow`). |
| `min()`, `max()` | lengths | One or more comma-separated sums. |
| `clamp(a, b, c)` | lengths | Exactly 3 arguments: `max(a, min(b, c))`. |
| `var(--name[, fallback])` | any value, any depth | See [Custom properties](#custom-properties). |
| `env(name[, fallback])` | any value | `safe-area-inset-top/right/bottom/left` give `Cascade::safe_area[i] / zoom` px (the fallback is ignored for these); any other name gives its fallback, or `0px` without one. |
| `rgb()`, `rgba()`, `hsl()`, `hsla()`, `color-mix()` | colours | Above. |
| `url()` | `background`, `background-image` | One string (quoted or raw). |
| `minmax()`, `repeat()` | grid tracks | Above. |
| `translate*()`, `scale*()`, `rotate()` | `transform` | Above. |
| `cubic-bezier()`, `steps()` | timing | Read as `ease` / `linear`. |
| `attr()`, `counter()`, `linear-gradient()` and other gradients, `image-set()`, `round()`, `mod()`, `sin()` and other math functions, `abs()`, `sign()`, `calc-size()` | | Not supported (the declaration is rejected, or for `background` ignored). |

## Custom properties

* Any name starting with `--` is known; its case is kept; its value is stored as component values.
* They inherit (`ComputedStyle::custom` is copied from the parent and extended).
* Resolution: each custom property's own `var()` references are substituted at the element that declares it (against the parent's resolved values and the element's other custom declarations, whichever won the cascade). The result is stored; descendants inherit the resolved value. A custom property whose substitution fails (missing variable without fallback, cycle, depth over 16) is removed from the map, even if the parent had a value.
* `var(--x)` inside a non-custom property: replaced by the element's resolved value; if missing, the fallback (everything after the first comma, which may contain commas) is substituted; if there is neither, **the whole declaration is dropped** (no `initial`/`inherit` fallback).
* The value is textual: `--w: 2em` stays `2em` and is converted where used (so `em` follows the using property's element).
* `@property` registration and typed custom properties are not supported.

## Media queries

Prelude: comma-separated queries, each `[not|only] [all|screen|print|speech] [and] (feature)...`.

| Feature | Forms | Evaluates against |
|---|---|---|
| `width`, `min-width`, `max-width`, `height`, `min-height`, `max-height` | `(min-width: 600px)`; value `px`, `em`/`rem` (x16), or `0`. Equality compares within 0.01 | `MediaEnv::width` / `height` (CSS px) |
| Range syntax | `(width >= 600px)`, `(600px <= width)`, `(400px < width <= 700px)`; operators `<`, `<=`, `>`, `>=`, `=`; only `width` and `height` (other names are false) | same |
| `prefers-color-scheme` | `dark`, `light` | `color_scheme` |
| `prefers-reduced-motion` | `reduce`, `no-preference` | `reduced_motion` |
| `hover`, `any-hover` | `hover`, `none`, or bare `(hover)` (true if hover) | `hover` (any-hover is the same flag) |
| `pointer`, `any-pointer` | `fine`, `coarse`, `none`; bare `(pointer)` is true unless `Pointer::None` | `pointer` (any-pointer is the same value) |
| `orientation` | `portrait` (height >= width), `landscape` (width > height) | size |
| `not (feature)` inside a parenthesised condition | `(not (hover))` | |
| Media types | `all`, `screen` match; `print`, `speech` never match; `only` skipped | |
| Anything else | `resolution`, `aspect-ratio`, `color`, `display-mode`, `prefers-contrast`, `forced-colors`, `scripting`, `update`, `or` conditions, a media feature with unknown value | **false** (the query does not match) |

A malformed query part (unknown media type word, `not` after the first position, a stray token) matches nothing; an `or` between conditions is malformed. Units other than `px`, `em`, `rem` in a feature value make that condition false.

## Cascade

| Aspect | Rule |
|---|---|
| Origins | One: author. There is no user-agent sheet (an unstyled `div` is `display: inline`, the engine's `Default`), no user origin, no `revert`. |
| Order of precedence | Sorted ascending, last wins: (1) `!important` false before true; (2) specificity; (3) rule `order`; (4) position of the declaration within the rule. Inline (`compute_with`) declarations use the maximum specificity and order, so they beat every rule of the same importance; `!important` inline beats `!important` in a rule. |
| Specificity | `(ids << 16) \| (classes << 8) \| tags`, each part capped at 255. ids: `#id`; classes: `.class`, `[attr]`, any single-colon pseudo-class (known or not); tags: type selectors; `:not()` counts as its most specific argument; `*`, `::pseudo`, `&` and unsupported functional pseudos add 0. |
| Source order | By rule `order` (increasing counter, see [Internals](internals.md#rule-order)); later wins ties. Across sheets: sheets added later are meant to win ([quirk](internals.md#order-across-sheets)). |
| Layers, scope | `@layer` has no ordering effect; `@scope` is ignored. |
| Media | A rule applies when every `@media` level around it holds in `Cascade::media`. |
| Inheritance | `color`, `font-family`, `font-size`, `font-weight`, `font-style` (`italic`), `line-height`, `text-align`, `text-transform`, `list-style`, `visibility` and custom properties. All else resets to the initial value per element. `touch-action` is not inherited (combine along the ancestors yourself with `TouchAction::and`). |
| Shorthand vs longhand | They apply in cascade order like any two declarations; there is no "shorthand resets only its longhands" bookkeeping beyond what each shorthand assigns. |
| Font size | `font`/`font-size` are applied first with the parent's `em`; every other property then sees the element's final font size as `em`. |
| After the cascade | Border and outline widths are zeroed where the style is `none`; absolutely positioned and fixed boxes are blockified (`inline`, `inline-block`, `list-item` become `block`; `inline-flex` becomes `flex`; `inline-grid` stays). |
