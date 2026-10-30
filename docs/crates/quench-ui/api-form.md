---
title: Form API
parent: quench-ui
nav_order: 3
---

Reference of `quench_ui::form` (source: `quench-ui/src/form.rs`): fields that hold a value and its error, rules that check them, and a form that checks every field before it submits. Worked examples are in [usage-app](usage-app.md).

`Field` and `Form` hold signals and `Rc`s: they are `!Send` and `!Sync`, cheap to clone, and **clones share state**. Create them inside a reactive root (they call `create_signal`).

```rust
use quench_ui::form::{Field, Form, Rule, rules};
```

## Rule

```rust
pub type Rule = std::rc::Rc<dyn Fn(&str) -> Option<String>>;
```

Checks a value: `Some(message)` when it is not acceptable, `None` when it is. A rule can be written by hand: `Rc::new(|v| (v != "yes").then(|| "Type yes".to_string()))`.

## rules

`pub mod rules`: ready-made rules. Every function is `#[must_use]`, takes the error message as `&str` and returns a `Rule`. A rule that says "an empty value passes" is meant to be combined with `required`.

| Function | Signature | Fails when |
|---|---|---|
| `required` | `pub fn required(message: &str) -> Rule` | The value is empty or only whitespace (`str::trim().is_empty()`). |
| `min_len` | `pub fn min_len(min: usize, message: &str) -> Rule` | The value is not empty and has fewer than `min` `char`s. (An empty value passes.) |
| `max_len` | `pub fn max_len(max: usize, message: &str) -> Rule` | The value has more than `max` `char`s. |
| `date` | `pub fn date(message: &str) -> Rule` | The value is not empty and is not `YYYY-MM-DD`: exactly three `-`-separated parts of 4, 2 and 2 ASCII digits, month 1 to 12, day within the month (Gregorian leap years: divisible by 4 and not by 100, or by 400). Year 0000 is accepted. |
| `time` | `pub fn time(message: &str) -> Rule` | The value is not empty and is not `HH:MM`: two two-character numbers, hour at most 23, minute at most 59. |
| `check` | `pub fn check(ok: impl Fn(&str) -> bool + 'static, message: &str) -> Rule` | The value is not empty and `ok(value)` is false. (An empty value passes.) |

`date` and `time` are built with `check`, so they too pass an empty value. A time written `7:05` fails (the hour must have two characters); a seconds part (`07:05:30`) fails.

Length is measured in `char`s (Unicode scalar values), not bytes or grapheme clusters.

## Field

```rust
#[derive(Clone)]
pub struct Field { /* private */ }
impl Debug for Field   // shows the name only
```

One input's value, its error, and whether the user has been through it (`touched`). The `touched` flag is set when the bound control loses focus and cleared by `reset`.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new(name: &str, initial: &str) -> Self` | A field called `name` with the value `initial`, no rules, no error, untouched. `name` becomes the `id` and `name` attributes of the bound control, and is what a server's error refers to (`Form::apply_errors`). Use a name that is unique on the page. |
| `rule` | `#[must_use] pub fn rule(self, rule: Rule) -> Self` | Adds a rule. Rules run in order; the first message wins. |
| `name` | `#[must_use] pub fn name(&self) -> &str` | The name. |
| `value` | `#[must_use] pub fn value(&self) -> String` | The value; a read in a computation follows changes. |
| `value_untracked` | `#[must_use] pub fn value_untracked(&self) -> String` | The value without subscribing. |
| `set` | `pub fn set(&self, value: &str)` | Changes the value (in a `batch`). Once the field is touched the new value is validated at once; before that the error is **cleared** (so an error from the server disappears when the user types). |
| `error` | `#[must_use] pub fn error(&self) -> Option<String>` | The current error (reactive). |
| `set_error` | `pub fn set_error(&self, message: &str)` | Shows `message` as the error (a server's verdict). The next `set` clears it (untouched) or replaces it by the validation result (touched). |
| `validate` | `#[must_use] pub fn validate(&self) -> bool` | Runs the rules on the current value, shows the first error (`set_if_changed`), returns true when there is none. With no rules it clears the error and returns true. |
| `reset` | `pub fn reset(&self, initial: &str)` | Value back to `initial`, error cleared, untouched (in a `batch`). |
| `input` | `#[must_use] pub fn input(&self) -> Element` | `self.bind(input())`. |
| `bind` | `#[must_use] pub fn bind(&self, control: Element) -> Element` | Binds any element that has a `value` (a `textarea`, a `select`) to the field; see below. |
| `error_view` | `#[must_use] pub fn error_view(&self) -> Element` | The error element; see below. |

### What `bind` / `input` emit

On the control passed in (`<input>` for `input()`):

| Attribute / listener | Value |
|---|---|
| `id` | the field name |
| `name` | the field name |
| `value` | dynamic: follows `Field::value()` |
| `aria-invalid` | `"true"` while there is an error, absent otherwise |
| `aria-describedby` | `"<name>-error"` while there is an error, absent otherwise (the id of `error_view`) |
| `input` listener | calls `Field::set(value)` unless an input method is composing |
| `compositionstart` | marks the field as composing |
| `compositionend` | clears the composing mark and calls `Field::set` with the final text (`Event::CompositionEnd(v)`) |
| `blur` | marks the field touched and validates it |

Nothing is taken from the control while an input method composes, so a half-typed syllable is not stored as a value. The `type`, `placeholder` and so on are the caller's to add on the returned `Element`.

### What `error_view` emits

```html
<p id="NAME-error" role="alert" class="q-field-error">MESSAGE</p>
```

The text is the error, or empty when there is none (the element is always in the page). A screen reader announces it when its text appears.

## Form

```rust
#[derive(Clone)]
pub struct Form { /* private */ }
impl Default for Form   // = Form::new()
impl Debug for Form     // opaque
```

Fields that are submitted together, and the state of the submission (a form-level error and a "submitting" flag).

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub fn new() -> Self` | An empty form. |
| `field` | `#[must_use] pub fn field(self, field: &Field) -> Self` | Adds a (clone of a) field. Order is the order of validation. |
| `validate` | `#[must_use] pub fn validate(&self) -> bool` | Validates **every** field (in a `batch`, so every error shows, not only the first); true when all pass. |
| `error` | `#[must_use] pub fn error(&self) -> Option<String>` | The form's own error (reactive): "wrong password", "offline". |
| `submitting` | `#[must_use] pub fn submitting(&self) -> bool` | Whether a submission is under way (reactive; disable the button while true). |
| `set_submitting` | `pub fn set_submitting(&self, on: bool)` | Sets the flag. The app sets it to true when it starts the request; `apply_errors` sets it to false. |
| `apply_errors` | `pub fn apply_errors(&self, message: &str, fields: &[(String, String)])` | Shows a server's verdict (below). |
| `clear_errors` | `pub fn clear_errors(&self)` | Clears the form's error and every field's error (batched). |
| `element` | `#[must_use] pub fn element(&self, on_valid: impl Fn(&Self) + 'static) -> Element` | The `<form>`; see below. |
| `error_view` | `#[must_use] pub fn error_view(&self) -> Element` | The form's own error; see below. |

### apply_errors

`fields` is a list of `(field name, message)`. Effects, in one `batch`:

1. `submitting` becomes false.
2. For each pair whose name is a field of this form, that field's error is set (`Field::set_error`).
3. A pair whose name is **not** a field of this form is "unplaced".
4. The form's error becomes `message` when `message` is not empty and either `fields` is empty or some pair was unplaced; otherwise it becomes `None`.

So the field-level message is shown on the field alone, and the general `message` appears on the form only when nothing field-specific could carry it. `apply_errors("offline", &[])` sets the form error to `offline`. `apply_errors("", &[...])` never sets a form error. Earlier field errors that are not named are left as they were (call `clear_errors` first if needed).

### What `element` emits

A `<form novalidate>` (the browser's own validation bubbles are off). Its `submit` listener, in order: returns at once if `submitting()` is true (a double submit is ignored); clears the form error; calls `validate()`; calls `on_valid(&form)` only if all fields passed. The handler does not set `submitting` itself: call `form.set_submitting(true)` in `on_valid`. `quench-ui` itself does nothing to stop the browser's default submit; the browser backend's `quench.js` prevents a `submit` event by default (`mode ... ?? (type === "submit" ? 1 : 0)`), so a form never reloads the page there. Other backends decide for themselves.

Add children with `.child(...)`: the bound inputs, `error_view()`s and a submit `button`.

### What `Form::error_view` emits

```html
<p role="alert" class="q-form-error">MESSAGE</p>
```

Empty text when there is no error. It has no `id`.
