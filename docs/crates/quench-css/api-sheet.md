---
title: "API: sheet"
parent: "quench-css"
nav_order: 7
---

Module `sheet`: stylesheet parsing. Nothing in this module fails: input that cannot be understood is skipped, as a browser does, and (where noted) recorded in `Stylesheet::ignored`.

## `Declaration`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub name: String,        // lowercase, except a custom property (`--x`), whose case is kept
    pub value: Vec<Value>,
    pub important: bool,
}
```

`name: value`. `value` is the component values with a trailing `! important` removed (`!` and `important` in any case, whitespace allowed between). The value is stored unsubstituted: `var()` and `env()` are still there.

## `Rule`

```rust
#[derive(Debug, Clone)]
pub struct Rule {
    pub selector: Complex,
    pub specificity: Specificity,
    pub declarations: Rc<Vec<Declaration>>,
    pub media: Vec<Vec<MediaQuery>>,
    pub order: usize,
}
```

One selector with its declarations: a rule with a selector list of three selectors is three `Rule`s sharing one `declarations` `Rc` and one `order`. `media` holds the `@media` conditions around the rule, outermost first; every level must hold, and a level holds when any of its queries matches ([`all_match`](api-media.md#all_match)). `order` is a source-order counter: see [Internals, rule order](internals.md#rule-order). Rules without declarations are not created.

## `Keyframes`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframes {
    pub name: String,
    pub frames: Vec<(f32, Vec<Declaration>)>,
}
```

A named animation. `frames` are `(offset, declarations)`, offset 0 to 1 (`from` is 0, `to` is 1, `p%` is `p / 100`; a selector list `0%, 50%` yields one entry per offset), sorted ascending by offset (stable for equal offsets). A frame selector that is not `from`, `to` or a percentage is dropped silently (the frame's block is consumed). A `@keyframes` whose name is neither an identifier nor a string is dropped.

## `Stylesheet`

```rust
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub keyframes: Vec<Keyframes>,
    pub ignored: Vec<String>,
}
```

`rules` are sorted by `order` (stable). `ignored` records what was left out, one string per event, in these exact formats:

| Entry | When |
|---|---|
| `@<name>` | An at-rule the parser does not handle (lowercase name, without the `@`), with or without a block: `@import`, `@charset`, `@font-face`, `@page`, `@property`, `@namespace`, `@layer a, b;` (statement form), `@media` without a block. |
| ``selector in `<text>` `` | A selector list had fewer parseable selectors than commas + 1. `<text>` is a rendering of the prelude tokens. |
| ``nesting in `<text>` `` | `resolve_nesting` returned `None` for a nested selector (see [`resolve_nesting`](api-selector.md#resolve_nesting)). |
| ``` `&` outside a rule in `<text>` ``` | A selector with `&` in a rule that has no parent rule. |
| ``declaration `<text>` `` | A statement that does not start with an identifier (for instance `*zoom: 1`, `: red`, `;` is skipped). |
| `property <name>` | A declaration whose name is not in `SUPPORTED`, `IGNORED` or a custom property. The declaration is **kept** in the rule (and does nothing). |

A statement that starts with an identifier but has no colon is dropped silently (no entry).

## `parse`

```rust
#[must_use]
pub fn parse(text: &str) -> Stylesheet
```

Parses a whole stylesheet.

* Statements: a statement ends at the first `;`, `{` or `}` outside brackets; `{` makes it a rule or at-rule, anything else a declaration.
* Declarations directly at the top level (outside any rule) are parsed (and unknown ones logged) and then discarded.
* Rules: selectors parse as in [`parse_selector_list`](api-selector.md#parse_selector_list). Nested rules and at-rules inside the block are resolved against every parent selector (cross product of parent and nested selector lists) and become top-level `Rule`s. Declarations of the block (before, between or after nested rules) all belong to the rule itself. A rule whose selector list ended up empty produces no rules, nested ones included.
* At-rules handled: `@media` (levels accumulate when nested), `@supports` (evaluated now; a true condition inlines the body, a false one drops it), `@keyframes` and `@-webkit-keyframes`, `@layer name { ... }` and `@container ... { ... }` (the body is inlined unconditionally; the layer name and the container condition are ignored).
* `@media` directly inside a rule: the rule's own declarations in that block become rules with the parents' selectors under the added media level.
* `@supports` truth: `(property: value)` is true when `is_known(property)` (the value is not examined); combined with `not`, `and`, `or` (left to right, no precedence) and nested parentheses. Anything else (`selector(...)`, a function, a bare word) makes the whole condition false.

```rust
use quench_css::parse;

let sheet = parse(".card { color: red; &:hover { color: blue } .title { margin: 0 } }");
assert_eq!(sheet.rules.len(), 3);   // .card, .card:hover, .card .title
assert!(sheet.ignored.is_empty());
```

## `parse_declarations`

```rust
#[must_use]
pub fn parse_declarations(text: &str) -> Vec<Declaration>
```

Parses the contents of a `style` attribute (`color: red; width: 10px`). Same tolerance as `parse`; rules found inside are parsed and discarded; unknown properties are kept (the log is not returned).
