---
title: Widgets
parent: quench-web-components
grand_parent: Crates
nav_order: 3
---

Reference for `buttons`, `containers`, `forms` and `status` (source: `src/{buttons,containers,forms,status}/mod.rs`). All are glob re-exported at the crate root (`quench_web_components::ButtonBuilder`) and available at their module path (`quench_web_components::buttons::ButtonBuilder`). Every builder is a consuming builder: methods take `self` and return `Self`; `build` returns `quench_web::Element`. Strings are accepted as `impl Into<String>`.

Text and attribute values are escaped by `Element` (`text` and `attr`).

## `buttons`

### `ButtonVariant`

```rust
#[derive(Clone, Copy, Debug)]
pub enum ButtonVariant { Primary, Secondary, Danger, Success, Warning, Outline }

impl ButtonVariant {
    #[must_use]
    pub const fn class_name(&self) -> &'static str
}
```

`class_name` returns `"btn btn-primary"`, `"btn btn-secondary"`, `"btn btn-danger"`, `"btn btn-success"`, `"btn btn-warning"`, `"btn btn-outline"` respectively.

### `ButtonBuilder`

```rust
#[derive(Clone, Debug, Default)]
pub struct ButtonBuilder { /* text, variant, disabled, button_type, id */ }
```

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `pub fn new(text: impl Into<String>) -> Self` | `button_type` = `"button"`, no variant, enabled, no id. (`ButtonBuilder::default()` has an **empty** `button_type`, which renders `type=""`.) Not `#[must_use]`. |
| `variant` | `#[must_use] pub const fn variant(self, variant: ButtonVariant) -> Self` | sets the variant |
| `primary` / `secondary` / `danger` / `success` / `warning` / `outline` | `#[must_use] pub const fn primary(self) -> Self` etc. | shortcut for `variant(ButtonVariant::X)` |
| `disabled` | `#[must_use] pub const fn disabled(self, disabled: bool) -> Self` | `true` writes `disabled="disabled"` |
| `button_type` | `#[must_use] pub fn button_type(self, button_type: impl Into<String>) -> Self` | the `type` attribute (`"submit"`, ...) |
| `id` | `#[must_use] pub fn id(self, id: impl Into<String>) -> Self` | the `id` attribute |
| `build` | `#[must_use] pub fn build(self) -> Element` | `<button type=".." [id] [disabled]>` with `class` = the variant's `class_name`, or `btn` without a variant; text is the label |

Helpers (none are `#[must_use]`): `pub fn primary_button(text: impl Into<String>) -> Element`, `secondary_button`, `danger_button`, `success_button`, `warning_button`, `outline_button`; each is `ButtonBuilder::new(text).<variant>().build()`.

```rust
let b = ButtonBuilder::new("Save").primary().button_type("submit").id("save").build();
assert_eq!(b.render(), r#"<button class="btn btn-primary" id="save" type="submit">Save</button>"#);
```

## `containers`

### `Card`

```rust
#[derive(Clone, Debug, Default)]
pub struct Card { /* title, content, footer, compact */ }
```

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new() -> Self` |
| `title` | `#[must_use] pub fn title(self, title: impl Into<String>) -> Self` |
| `content` | `#[must_use] pub fn content(self, content: Element) -> Self` |
| `footer` | `#[must_use] pub fn footer(self, footer: Element) -> Self` |
| `compact` | `#[must_use] pub const fn compact(self, compact: bool) -> Self` |
| `build` | `#[must_use] pub fn build(self) -> Element` |

Output: `div.card` (plus `card-compact` when compact), containing, only for the parts that are set and in this order: `div.card-header > h3(title)`, `div.card-body > content`, `div.card-footer > footer`.

### `Panel`

```rust
#[derive(Clone, Debug, Default)]
pub struct Panel { /* title, content, bordered, highlighted */ }
```

Methods: `new()`, `title(impl Into<String>)`, `content(Element)`, `bordered(bool)`, `highlighted(bool)`, `build()`; all `#[must_use]`, `bordered` and `highlighted` are `const fn`.

Output: `div.panel` plus `panel-bordered` and/or `panel-highlighted`; children `div.panel-header > h3(title)` and `div.panel-content > content` when set.

### `Container`

```rust
#[derive(Clone, Debug, Default)]
pub struct Container { /* content, fluid, centered */ }
```

Methods: `new()`, `content(Element)`, `fluid(bool)`, `centered(bool)`, `build()`; all `#[must_use]`.

Output: `div.container` plus `container-fluid` and/or `container-centered`; the content as the direct child when set.

### Helper functions

None is `#[must_use]` except `empty_state`.

| Function | Equivalent |
|---|---|
| `pub fn card(title: impl Into<String>, content: Element) -> Element` | `Card::new().title(title).content(content).build()` |
| `pub fn compact_card(title: impl Into<String>, content: Element) -> Element` | the same with `.compact(true)` |
| `pub fn panel(title: impl Into<String>, content: Element) -> Element` | `Panel::new().title(..).content(..).bordered(true).build()` |
| `pub fn highlighted_panel(title: impl Into<String>, content: Element) -> Element` | the same with `.highlighted(true)` |
| `#[must_use] pub fn empty_state(i18n_key: &str) -> Element` | `<div class="empty" data-i18n="{key}"></div>`; the "nothing here" placeholder. The wrapper (content/container) stays with the caller. `.empty` is the one class here that the shared stylesheet defines. |

## `forms`

All field builders produce a bare control when no label is set, and `div.form-group > [label[for=name], control]` when `.label(..)` was called. The control's `id` equals its `name`.

### `FormInput`

```rust
#[derive(Clone, Debug, Default)]
pub struct FormInput { /* name, label, input_type, placeholder, required, value */ }
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `pub fn new(name: impl Into<String>) -> Self` | `input_type` = `"text"`. Not `#[must_use]`. (`Default` leaves `input_type` empty.) |
| `label` | `#[must_use] pub fn label(self, label: impl Into<String>) -> Self` | |
| `input_type` | `#[must_use] pub fn input_type(self, input_type: impl Into<String>) -> Self` | `"password"`, `"email"`, ... |
| `placeholder` | `#[must_use] pub fn placeholder(self, placeholder: impl Into<String>) -> Self` | |
| `required` | `#[must_use] pub const fn required(self, required: bool) -> Self` | writes `required="required"` |
| `value` | `#[must_use] pub fn value(self, value: impl Into<String>) -> Self` | |
| `build` | `#[must_use] pub fn build(self) -> Element` | `<input type name id [placeholder] [value] [required]>` |

### `FormSelect`

```rust
#[derive(Clone, Debug, Default)]
pub struct FormSelect { /* name, label, options: Vec<(String, String)>, selected, required */ }
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `pub fn new(name: impl Into<String>) -> Self` | not `#[must_use]` |
| `label` | `#[must_use] pub fn label(self, label: impl Into<String>) -> Self` | |
| `option` | `#[must_use] pub fn option(self, value: impl Into<String>, label: impl Into<String>) -> Self` | appends `(value, label)` |
| `options` | `#[must_use] pub fn options(self, options: Vec<(String, String)>) -> Self` | **replaces** all options set so far |
| `selected` | `#[must_use] pub fn selected(self, value: impl Into<String>) -> Self` | every option whose value equals it gets `selected="selected"` |
| `required` | `#[must_use] pub const fn required(self, required: bool) -> Self` | |
| `build` | `#[must_use] pub fn build(self) -> Element` | `<select name id [required]>` with `<option value>label</option>` children |

### `FormTextarea`

```rust
#[derive(Clone, Debug, Default)]
pub struct FormTextarea { /* name, label, placeholder, rows, cols, required, value */ }
```

Methods: `pub fn new(name: impl Into<String>) -> Self` (not `#[must_use]`), then `#[must_use]` `label(impl Into<String>)`, `placeholder(impl Into<String>)`, `pub const fn rows(self, rows: usize)`, `pub const fn cols(self, cols: usize)`, `pub const fn required(self, required: bool)`, `value(impl Into<String>)` (becomes the element's text), `build()`.

Output: `<textarea name id [rows] [cols] [placeholder] [required]>value</textarea>`.

```rust
let f = FormInput::new("email").label("Email").input_type("email").required(true).build();
assert_eq!(
    f.render(),
    r#"<div class="form-group"><label for="email">Email</label><input id="email" name="email" required="required" type="email"></input></div>"#
);
```

## `status`

### `StatusLevel`

```rust
#[derive(Clone, Copy, Debug)]
pub enum StatusLevel { Info, Success, Warning, Error }

impl StatusLevel {
    #[must_use] pub const fn class_name(&self) -> &'static str
    #[must_use] pub const fn icon_class(&self) -> &'static str
}
```

| Level | `class_name` | `icon_class` |
|---|---|---|
| `Info` | `status-info` | `fas fa-info-circle` |
| `Success` | `status-success` | `fas fa-check-circle` |
| `Warning` | `status-warning` | `fas fa-exclamation-circle` |
| `Error` | `status-error` | `fas fa-times-circle` |

### `StatusBadge`

```rust
#[derive(Clone, Debug, Default)]
pub struct StatusBadge { /* text, level, with_icon */ }
```

| Method | Signature |
|---|---|
| `new` | `pub fn new(text: impl Into<String>) -> Self` (not `#[must_use]`) |
| `level` | `#[must_use] pub const fn level(self, level: StatusLevel) -> Self` |
| `info`, `success`, `warning`, `error` | `#[must_use] pub const fn info(self) -> Self` etc. |
| `with_icon` | `#[must_use] pub const fn with_icon(self, with_icon: bool) -> Self` |
| `build` | `#[must_use] pub fn build(self) -> Element` |

Output: `span.status-badge`; with a level it also gets the level's class and, only when `with_icon` is true, an `<i class="{icon_class}">` child; `Element` writes its text before its children, so the **icon comes after the text** in the markup (`<span ...>healthy<i ...></i></span>`). **Without a level, `with_icon` has no effect** (no icon is drawn).

### `AlertBox`

```rust
#[derive(Clone, Debug, Default)]
pub struct AlertBox { /* message, level, closeable */ }
```

Methods: `pub fn new(message: impl Into<String>) -> Self` (not `#[must_use]`); `#[must_use]` `level`, `info`, `success`, `warning`, `error`, `pub const fn closeable(self, closeable: bool)`, `build`.

Output: `div.alert` with, if a level is set, the class `alert-{class_name}` (so `alert-status-info`, `alert-status-error`, ...), containing `span(message)`, and when closeable `<span class="alert-close" role="button" aria-label="Close alert"><i class="fas fa-times"></i></span>`. The close span has no click handler: nothing closes the alert unless the page adds one.

### Helper functions (none `#[must_use]`)

| Function | Equivalent |
|---|---|
| `pub fn success_badge(text: impl Into<String>) -> Element` | `StatusBadge::new(text).success().with_icon(true).build()` |
| `pub fn warning_badge(text: impl Into<String>) -> Element` | warning, with icon |
| `pub fn error_badge(text: impl Into<String>) -> Element` | error, with icon |
| `pub fn info_alert(message: impl Into<String>) -> Element` | `AlertBox::new(message).info().build()` |
| `pub fn success_alert(message: impl Into<String>) -> Element` | success |
| `pub fn warning_alert(message: impl Into<String>) -> Element` | warning |
| `pub fn error_alert(message: impl Into<String>) -> Element` | error |

There is no `info_badge` helper (use `StatusBadge::new(..).info()`).
