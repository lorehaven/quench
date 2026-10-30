---
title: "API: selector"
parent: "quench-css"
nav_order: 6
---

Module `selector`: selector parsing, specificity, matching against any tree. What each selector kind does is tabulated on [Supported CSS](supported.md#selectors).

## `Element`

```rust
pub trait Element: Sized {
    fn tag(&self) -> String;
    fn id(&self) -> Option<String>;
    fn has_class(&self, class: &str) -> bool;
    fn attribute(&self, name: &str) -> Option<String>;
    fn parent(&self) -> Option<Self>;
    fn previous_sibling(&self) -> Option<Self>;
    fn is_root(&self) -> bool { self.parent().is_none() }
    fn state(&self, state: State) -> bool;
}
```

What a selector asks of one node of the tree. Implement it for a cheap handle (an index plus an `Rc` to the tree, as the tests do); the engine clones the result of `parent()` and `previous_sibling()` freely while matching.

| Method | Contract |
|---|---|
| `tag` | Lowercase tag name. Compared to the selector's tag with `eq_ignore_ascii_case`. |
| `id` | The `id` attribute. Compared exactly (case-sensitive). |
| `has_class(class)` | Whether `class` is one of the element's classes; the engine passes the class as written in the selector (case kept), the implementation decides case sensitivity. |
| `attribute(name)` | The attribute's value, or `None` when absent. `name` is lowercase (the parser lowercases attribute names). An attribute without a value should return `Some("")` for `[attr]` to match. |
| `parent` | The parent element, `None` for the root. Used by `>`, descendant matching and the default `is_root`. |
| `previous_sibling` | The element just before this one among its parent's *element* children, or `None` for the first. Used by `+`, `~` and `:first-child`. |
| `is_root` | Default: no parent. Used by `:root`. |
| `state` | Interaction state not in the markup (see `State`). |

## `State`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State { Hover, Focus, FocusVisible, Active, Disabled, Checked }
```

Asked of the tree by `:hover`, `:focus`, `:focus-visible`, `:active`, `:disabled` and `:checked`. `:disabled` and `:checked` are also true when the element has a `disabled` / `checked` attribute (any value); `:enabled` is the negation of the `:disabled` test.

## `Combinator`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator { Descendant, Child, NextSibling, LaterSibling }
```

Space, `>`, `+`, `~`.

## `AttrOp`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrOp { Exists, Equals, Includes, Prefix, Suffix, Substring, DashMatch }
```

`[a]`, `[a=v]`, `[a~=v]`, `[a^=v]`, `[a$=v]`, `[a*=v]`, `[a|=v]`. **Not re-exported from the crate root** (`lib.rs` does not list it), although `Simple::Attribute` has a field of this type; downstream code can read the field but cannot write the type's path. See the [gap report](../../_reports/ui-css.md).

## `Simple`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Simple {
    Id(String),
    Class(String),
    Attribute { name: String, op: AttrOp, value: String, ignore_case: bool },
    Pseudo(String),
    Not(Vec<Complex>),
    Unknown,
    Nesting,
}
```

One condition of a compound selector. `Pseudo(name)` holds a lowercase pseudo-class name *for any name written with a single colon and no parentheses*, known or not (`:before` becomes `Pseudo("before")` and never matches). `Unknown` is a pseudo-element (`::x`) or an unsupported functional pseudo-class (`:is()`, `:where()`, `:has()`, `:nth-child()`, ...); it never matches and adds no specificity. `Nesting` is `&` until nesting is resolved. `Attribute.value` is empty for `Exists`.

## `Compound`

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Compound {
    pub tag: Option<String>,   // lowercase; None = any element (`*` or absent)
    pub conditions: Vec<Simple>,
}
```

`button.q-chip[aria-pressed="true"]`.

## `Complex`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Complex {
    pub parts: Vec<(Combinator, Compound)>,
}
```

Compounds left to right. `a > b c` is `[(_, a), (Child, b), (Descendant, c)]`; the combinator of the first part is unused (it is `Descendant`, or whatever a leading combinator in the source set, as in nested `> li`).

### `Complex::specificity`

```rust
#[must_use]
pub fn specificity(&self) -> Specificity
```

`(ids.min(255) << 16) | (classes.min(255) << 8) | tags.min(255)`. Counts: `Id` is an id; `Class`, `Attribute` and `Pseudo` (any name) are classes; a compound with a tag counts one tag (`*` counts none); `Not(list)` counts as its most specific argument (the maximum of the list's specificities, split back into id/class/tag parts); `Unknown` and `Nesting` count nothing. So `ul > li:first-child` is `(1 << 8) | 2 = 258`.

### `Complex::matches`

```rust
#[must_use]
pub fn matches<E: Element>(&self, element: &E) -> bool
```

Right-to-left matching: the last compound must match `element`, then each combinator walks to the parent, ancestors (any, with backtracking), previous sibling or earlier siblings. A compound matches when its tag matches (case-insensitively) and every condition holds. `Not(list)` holds when no selector of the list matches (an empty list, from `:not()` whose arguments all failed to parse, holds always). Panics on a `Complex` with no parts (cannot come from the parser).

### `Complex::has_nesting`

```rust
#[must_use]
pub fn has_nesting(&self) -> bool
```

Whether a compound still holds a `Simple::Nesting` (not checked inside `:not()`).

## `Specificity`

```rust
pub type Specificity = u32;
```

Encoding above. Inline styles use `Specificity::MAX` in the cascade.

## `parse_selector_list`

```rust
#[must_use]
pub fn parse_selector_list(tokens: &[Token]) -> Vec<Complex>
```

Splits at top-level commas (not inside brackets or functions) and parses each part; a part that does not parse is left out, as a browser drops an invalid selector. Does not fail and does not report which part was dropped (the sheet parser counts commas to detect it).

A part fails (is dropped) for: an empty part; a `.` not followed by an identifier; `[` without `]`, an attribute without a valid operator or value, or an attribute value that is not an identifier, string or number; a `:` not followed by an identifier or function; a tag after other content in the same compound (`.a*`), or any token not listed on the [Supported](supported.md#selectors) page (a leading digit, `"strings"`, `%`, ...).

```rust
use quench_css::{parse_selector_list, tokenize};

let list = parse_selector_list(&tokenize("a:hover, 3bad, .x > .y"));
assert_eq!(list.len(), 2);          // `3bad` is dropped
assert_eq!(list[1].parts.len(), 2);
```

## `resolve_nesting`

```rust
#[must_use]
pub fn resolve_nesting(parent: &Complex, nested: &Complex) -> Option<Complex>
```

Resolves `nested` against `parent`:

* `&` in the first compound of `nested`: the first compound's tag and conditions (minus `&`) are merged into the *last* compound of `parent`. If both name a tag and the tags differ, the result is `None` (it could never match). If `parent`'s last compound has no tag, it takes the nested tag.
* No `&` anywhere: `nested` is appended to `parent`, keeping `nested`'s first combinator (so `> li` is a child, anything else a descendant).
* `&` in any later compound: `None`.

The rest of `nested` (parts after the first) is appended unchanged.
