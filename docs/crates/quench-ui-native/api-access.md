---
title: Access API
parent: quench-ui-native
nav_order: 2
---

Source: `src/access.rs`. The accessibility tree is what a screen reader is told about the page. Two bridges read it: the AT-SPI service of [quench-desktop-atspi](../quench-desktop-atspi/index.md) (Linux) and the node provider of [quench-android-surface](../quench-android-surface/index.md) (Android).

## Role

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Document, Heading, Paragraph, Text, Button, Link, Checkbox, Radio, Switch,
    TextBox, List, ListItem, Tab, TabList, TabPanel, Dialog, Navigation,
    Status, Alert, Image, Form, Group, Slider, ProgressBar,
}
```

24 variants. `Document` is the whole page (the root node only); `Text` is a piece of text with no other role; `Slider` is `input type=range` / `role=slider`; `ProgressBar` is `progress`, `meter`, `role=progressbar`. `Switch`, `TabList`, `TabPanel`, `Status`, `Alert` and `Group` are produced only from a `role` attribute (see below), never from a tag.

## AccessAction

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessAction {
    Click,          // press, check or follow
    Focus,
    ScrollIntoView, // scroll the node into view
}
```

## AccessNode

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    pub id: usize,                              // DOM node (NodeId::index); actions go to this element
    pub role: Role,
    pub level: u8,                              // h1-h6: 1-6; 0 otherwise
    pub name: String,                           // the accessible name
    pub description: String,                    // hint, error message
    pub value: Option<String>,                  // field text, or slider/progress value
    pub bounds: (f32, f32, f32, f32),           // (x, y, width, height) on the screen
    pub focusable: bool,
    pub focused: bool,
    pub disabled: bool,
    pub checked: Option<bool>,                  // checkbox, radio, switch
    pub selected: Option<bool>,                 // aria-pressed / aria-selected
    pub expanded: Option<bool>,                 // aria-expanded
    pub required: bool,
    pub read_only: bool,
    pub caret: Option<(usize, usize)>,          // focused text field: (caret, anchor) in characters
    pub range: Option<(f64, f64, Option<f64>)>, // (min, max, current); None current = indeterminate
    pub actions: Vec<AccessAction>,
    pub children: Vec<AccessNode>,
}
```

`Debug, Clone, PartialEq`; no `Default`, no builder.

### find

```rust
#[must_use]
pub fn find(&self, id: usize) -> Option<&Self>
```

The node with this DOM id, depth first (self first).

### count

```rust
#[must_use]
pub fn count(&self) -> usize
```

Number of nodes in the subtree including self.

## NativeRenderer::accessibility_tree

```rust
#[must_use]
pub fn accessibility_tree(&self) -> AccessNode
```

Lays the document out first when there is no layout yet or the `dirty` flag is set (note: see [report](../../_reports/ui-native.md) on `dirty` versus `stale`), then walks the DOM from `<body>` (node 1). The root is:

* `id: 0`, `role: Role::Document`, `name`: the `aria-label` of `<body>` (else empty), `bounds: (0, 0, viewport.0, viewport.1)`, no actions, children = the nodes found under body.

## NativeRenderer::perform

```rust
pub fn perform(&self, id: usize, action: AccessAction) -> bool
```

Carries out what an assistive technology asked for on DOM node `id`. Returns `false` when there is no layout yet, `id` is not an element (text, anchor, out of range: `tag(id)` is `None`), or (for `Click`/`ScrollIntoView`) the element has no box.

* `Focus`: keyboard focus on the node (no focusability check); returns `true`.
* `Click`: `pointer_down` then `pointer_up` at the centre of the node's on-screen rectangle (so the click goes to whatever is on top there, bubbling to listeners, with hover/active/focus side effects); `true`.
* `ScrollIntoView`: if the rectangle is above the window (`y < 0`) scrolls up by that amount with `wheel(0,0,0,y)`; if it ends below the viewport scrolls down by the overflow; `true` (also when already visible). Scrolls the container at the top-left pixel, then the document.

`set_field_value` (assistive technology sets text) lives with the [input API](api-input.md#set_field_value).

## Which elements become nodes

Algorithm (`collect`, from `<body>` downward):

* `Anchor`: skipped.
* Text node: becomes a `Role::Text` node (name = text with whitespace collapsed to single spaces, bounds = parent's rectangle) only if non-empty after collapsing and its parent is *not* a role that names itself from content (below).
* Element:
  * Hidden (below): skipped with its subtree.
  * No role (`role_of` is `None`): the element is flattened; its children are collected into the parent. This is how a `div`/`span`/`section` that only groups disappears.
  * Otherwise a node with the attributes listed below. For roles that name themselves from content (`Button, Link, Heading, Tab, Switch, Checkbox, Radio`) the text children are not nodes (the text is the name) and only interactive descendants stay (nested `Button, Link, Checkbox, Radio, Switch, TextBox` through any wrapper elements). Other roles collect all children.

**Hidden** (`hidden`): `aria-hidden="true"`; a `hidden` attribute; the element has no box in the last layout and is not one of the inline-like tags (`span a b i em strong small code label br abbr mark u s sub sup`, which have no box of their own); or its box has `display: none` or `visibility` not visible.

## Role mapping

If the element has a `role` attribute it decides, else the tag:

| `role` value | `Role` |
|---|---|
| `button`, `link`, `checkbox`, `radio`, `switch`, `tab`, `tablist`, `tabpanel`, `navigation`, `status`, `alert`, `slider`, `progressbar`, `list`, `listitem` | the same-named role |
| `textbox`, `searchbox` | `TextBox` |
| `dialog`, `alertdialog` | `Dialog` |
| `img`, `image` | `Image` |
| `heading` | `Heading`, level = `aria-level` parsed as `u8`, default 2 |
| `presentation`, `none` | no role (flattened) |
| any other value | `Group` |

| Tag (no `role`) | `Role` |
|---|---|
| `h1`-`h6` | `Heading` (level 1-6) |
| `p` | `Paragraph` |
| `button` | `Button` |
| `a` with an `href` | `Link` (an `a` without `href` has no role) |
| `input` type `checkbox` | `Checkbox` |
| `input` type `radio` | `Radio` |
| `input` type `range` | `Slider` |
| `input` type `button`, `submit`, `reset` | `Button` |
| `input` type `hidden` | none |
| `input` any other type (missing type = `text`) | `TextBox` |
| `textarea`, `select` | `TextBox` |
| `progress`, `meter` | `ProgressBar` |
| `ul`, `ol` | `List` |
| `li` | `ListItem` |
| `nav` | `Navigation` |
| `form` | `Form` |
| `img` | `Image` |
| `dialog` | `Dialog` |
| anything else | none |

## Attribute to node-field mapping

| Field | Source |
|---|---|
| `id` | DOM index |
| `level` | heading level above, else 0 |
| `name` | see below |
| `description` | `aria-describedby`: text of every element whose `id` is listed (space-separated), joined by a space; if the attribute is absent, `aria-description`; else empty |
| `bounds` | the element's rectangle on the screen: `translate()` applied, scroll offsets of clipping ancestors subtracted, `position: fixed` boxes added back by the scroll so they stay where they are; `(0,0,0,0)` if it has no box |
| `disabled` | `disabled` attribute present, or `aria-disabled="true"` |
| `focusable` | not disabled and the element is [focusable](api-input.md#focus) |
| `focused` | it is the focused element |
| `required` | `required` attribute present or `aria-required="true"` |
| `read_only` | `readonly` attribute present (note: `aria-readonly` is not read) |
| `checked` | `Checkbox`, `Radio`: `Some(checked attribute present \|\| aria-checked="true")`; `Switch`: `Some(aria-checked == "true")`; else `None` |
| `selected` | `Some(v != "false")` of `aria-pressed`, else of `aria-selected`; `None` if neither present |
| `expanded` | `Some(v != "false")` of `aria-expanded`, `None` if absent |
| `value` | text field: the field's value (`password` values are *not* masked); `Slider`/`ProgressBar`: the current value as text (whole numbers without decimals) |
| `caret` | text field that is focused: `Some((caret, anchor))` in characters |
| `range` | `Slider`/`ProgressBar`: see below |
| `actions` | `Click` for roles `Button, Link, Checkbox, Radio, Switch, Tab` when not disabled; `Focus` when focusable and not disabled; `ScrollIntoView` always |

Text field means `textarea` or `input` of type `text`, `email`, `password`, `search`, `tel`, `url`, `number`. `select` has role `TextBox` but is not a text field (no value, no caret).

### Accessible name

First non-empty of:

1. `aria-label`.
2. `aria-labelledby`: the text content of each element whose `id` is listed, joined by a space (unknown ids skipped).
3. For roles `TextBox`, `Checkbox`, `Radio` and any `input`: the text of the label: a `<label>` ancestor, else a `<label for="id">` naming the element's `id`.
4. For roles `Button, Link, Heading, Tab, Switch, ListItem, Checkbox, Radio`: the element's text content (whitespace collapsed, hidden parts and `input` elements left out).
5. `title`.
6. `alt`.
7. `placeholder`.
8. For `input` of type `button|submit|reset`: its `value`.
9. Empty string.

### Range and value of sliders and progress bars

`range_of(role)`: `min` = `min` attribute, else `aria-valuemin`, else 0. `max` = `max`, else `aria-valuemax`, else the default (100 for sliders and meters, 1 for `progress`), and never below `min`. `now` = `value`, else `aria-valuenow`, clamped to `[min, max]`; numbers are `f64` parsed from the trimmed text. A slider with no value sits in the middle (`min + (max - min) / 2`); a progress bar or meter with no value has `now: None` (indeterminate; nothing is drawn).

## Example

```rust
let tree = r.accessibility_tree();
let ok = tree.find(button_index).unwrap();
assert_eq!(ok.role, Role::Button);
assert!(ok.actions.contains(&AccessAction::Click));
r.perform(button_index, AccessAction::Click);
```

See `tests/unit.rs::the_accessibility_tree_has_roles_names_states_and_places` for a full walk.
