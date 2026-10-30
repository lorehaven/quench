---
title: Component kit API
parent: quench-ui
nav_order: 5
---

Reference of `quench_ui::kit` (source: `quench-ui/src/kit.rs`): components for touch-first apps. Each is a plain function that returns an `Element` (or a `View`) with `q-*` classes; the `quench-style` crate (`components` module) styles them. Each carries the ARIA roles and keyboard handling its pattern needs. The crate itself ships no CSS: the classes below are hooks.

Everything is single-threaded (`!Send`). Signals passed in (`&Signal<bool>`, `&Signal<usize>`, `&Signal<Option<String>>`) are cloned inside; the component owns no state of its own except where stated. Components that use `presence` or `toast` need a [platform](api-platform.md) installed for timing; without one, `after` runs at once.

```rust
use quench_ui::kit::{
    NavItem, PULL_THRESHOLD, SWIPE_REVEAL, app_bar, bottom_nav, checkbox, chip, date_field, dialog,
    fab, icon_button, list, list_tile, pull_to_refresh, sheet, snackbar, swipe_action, switch, tabs,
    text_field, tile, time_field, toast,
};
```

Notation: "emits" lists the elements as `tag.class[attributes]`. Attribute order is not significant. Names of `aria-*` attributes are lower-cased by the view layer.

## Index

| Function | Returns | Section |
|---|---|---|
| `app_bar` | `Element` | [App bar](#app_bar) |
| `icon_button` | `Element` | [Icon button](#icon_button) |
| `NavItem`, `bottom_nav` | `Element` | [Bottom navigation](#bottom_nav) |
| `sheet`, `dialog` | `View` | [Sheet and dialog](#sheet-and-dialog) |
| `snackbar`, `toast` | `View` / `()` | [Snackbar](#snackbar-and-toast) |
| `list`, `list_tile`, `tile` | `Element` / `View` / `Element` | [Lists](#lists) |
| `switch`, `checkbox`, `chip` | `Element` | [Toggles](#toggles) |
| `text_field`, `date_field`, `time_field` | `Element` | [Fields](#fields) |
| `fab` | `Element` | [Floating action button](#fab) |
| `tabs` | `Element` | [Tabs](#tabs) |
| `PULL_THRESHOLD`, `pull_to_refresh` | `f64`, `Element` | [Pull to refresh](#pull_to_refresh) |
| `SWIPE_REVEAL`, `swipe_action` | `f64`, `Element` | [Swipe action](#swipe_action) |

## app_bar

```rust
#[must_use]
pub fn app_bar(title: &str, leading: Option<View>, actions: Vec<View>) -> Element
```

The bar across the top of a screen: a title, an optional leading control (a back or menu button) and trailing actions.

Emits:

```text
header.q-appbar
  [leading view]                      (if Some)
  h1.q-appbar-title   "title"
  div.q-appbar-actions  [actions...]
```

## icon_button

```rust
#[must_use]
pub fn icon_button(label: &str, glyph: &str, on_click: impl Fn() + 'static) -> Element
```

A square button with a symbol. `glyph` is decorative text (an emoji or a symbol character); `label` is the accessible name.

Emits `button.q-iconbtn[type="button" aria-label=label]` containing `span[aria-hidden="true"]` with `glyph`. `on_click` is the click handler.

## NavItem and bottom_nav

```rust
#[derive(Clone, Debug)]
pub struct NavItem {
    pub label: String,
    pub glyph: String,
}

impl NavItem {
    #[must_use]
    pub fn new(label: &str, glyph: &str) -> Self
}

#[must_use]
pub fn bottom_nav(label: &str, items: Vec<NavItem>, selected: &Signal<usize>) -> Element
```

Navigation between a few top-level destinations along the bottom of the screen. `selected` is the index of the current one; a click on item `i` sets it to `i`. `label` is the accessible name of the `nav`.

Emits:

```text
nav.q-bottomnav[aria-label=label]
  button.q-bottomnav-item[type="button" aria-current="page" (only on the selected one)]
    span.q-bottomnav-glyph[aria-hidden="true"]  glyph
    span.q-bottomnav-label                      label
  ...
```

The marker is `aria-current`, not `role="tab"`; there is no extra key handling (native buttons, Tab order). An index outside the list simply marks nothing.

## sheet and dialog

```rust
#[must_use]
pub fn sheet(open: &Signal<bool>, title: &str, content: impl Fn() -> View + 'static) -> View

#[must_use]
pub fn dialog(
    open: &Signal<bool>,
    title: &str,
    content: impl Fn() -> View + 'static,
    actions: impl Fn() -> Vec<View> + 'static,
) -> View
```

`sheet` is a panel that slides up from the bottom, over the page, while `open` is true. `dialog` is the same, centred, with `actions` (buttons) along the bottom. Both are built with [`presence`](api-platform.md#presence): the modal stays in the page until its exit transition has played (`sheet`: duration 250 ms, `dialog`: 200 ms), so they are wrapped in `div.q-anim` / `.q-anim-in`. `content` and `actions` are called each time the modal is shown (a fresh view each time).

Both close by setting `open` to false on: a click on the scrim, and `Escape` pressed while focus is inside the modal. They trap focus (Tab and Shift+Tab cycle inside; focus moves into the modal when it is built). A host with a back button should close the sheet from `Router::on_back` ([api-router](api-router.md#router)):

```rust
router.on_back({ let open = open.clone(); move || { if open.get_untracked() { open.set(false); true } else { false } } });
```

Emits (inside the presence wrapper `div.q-anim[.q-anim-in]`):

```text
div.q-modal
  div.q-scrim                         (click: open = false)
  div.q-sheet | div.q-dialog          [role="dialog" aria-modal="true" aria-label=title tabindex="-1"]
    h1.q-modal-title  title
    <content>                         (sheet)
    div > <content> + div.q-dialog-actions > <actions...>   (dialog)
```

The title appears twice: as `aria-label` and as the visible heading (`aria-labelledby` is not used). The heading is an `h1`.

## snackbar and toast

```rust
#[must_use]
pub fn snackbar(message: &Signal<Option<String>>, action: Option<(&str, Rc<dyn Fn()>)>) -> View

pub fn toast(message: &Signal<Option<String>>, text: &str, ms: u32)
```

`snackbar` is a brief message at the bottom of the screen. It shows while `message` holds `Some`; the optional `action` adds a button (label, handler) such as "Undo". Clearing `message` after a few seconds is the app's job; `toast` does it. It is built lazily (`View::lazy`) and shown through `presence` (200 ms). The text is remembered while the snackbar fades out after `message` became `None`, so it does not go blank. A `Some` message replaces the displayed text live.

Emits (inside `div.q-anim`):

```text
div.q-snackbar[role="status" aria-live="polite"]
  span.q-snackbar-text   <current message>
  button.q-snackbar-action[type="button"]  action label     (only with an action)
```

The action button calls the handler, then sets `message` to `None`.

`toast` sets `message` to `Some(text)` and, after `ms` milliseconds on the installed platform, clears it, but only if the message is still `text` (a different newer message is left alone). A later `toast` of the **same** text does not cancel the first one's timer, so the first timer can clear the second prematurely. Without an installed platform `platform::after` runs the clearing at once, so the message is immediately `None` again.

## Lists

```rust
#[must_use]
pub fn list(tiles: Vec<View>) -> Element

#[must_use]
pub fn list_tile(
    title: &str,
    subtitle: Option<&str>,
    leading: Option<View>,
    trailing: Option<View>,
    on_tap: Option<Rc<dyn Fn()>>,
) -> View

#[must_use]
pub fn tile(
    title: &str,
    subtitle: Option<&str>,
    leading: Option<View>,
    trailing: Option<View>,
    on_tap: Option<Rc<dyn Fn()>>,
) -> Element
```

`list` emits `ul.q-list` with the tiles as children (give it `list_tile` views; each is an `li`). `list_tile` is `li > tile(...)`. `tile` is the row of a `list_tile` without its `<li>`, for putting inside another element (a [`swipe_action`](#swipe_action)).

Row emitted by `tile`, in this child order:

```text
button.q-tile[type="button"]  (with on_tap, click calls it)   |   div.q-tile  (without)
  div.q-tile-leading   <leading>          (if Some)
  div.q-tile-text
    span.q-tile-title     title
    span.q-tile-subtitle  subtitle        (if Some)
  div.q-tile-trailing  <trailing>         (if Some)
```

## Toggles

### switch

```rust
#[must_use]
pub fn switch(label: &str, on: &Signal<bool>) -> Element
```

An on/off switch with a label. Emits `button.q-switch[type="button" role="switch" aria-checked="true"|"false"]` (dynamic) with `span.q-switch-track[aria-hidden="true"] > span.q-switch-thumb` and `span.q-switch-label`. A click flips `on`; Space and Enter on a focused button produce clicks natively.

### checkbox

```rust
#[must_use]
pub fn checkbox(text: &str, checked: &Signal<bool>) -> Element
```

Emits `label.q-checkbox` containing `input[type="checkbox"]` (the `checked` attribute is present while the signal is true; a click flips the signal) and `span.q-checkbox-label` with `text`. Keyboard and screen-reader support are the platform's.

### chip

```rust
#[must_use]
pub fn chip(text: &str, pressed: &Signal<bool>) -> Element
```

A toggle chip (a filter). Emits `button.q-chip[type="button" aria-pressed="true"|"false"]` with the extra class `q-chip-on` while pressed. A click flips the signal.

## Fields

```rust
#[must_use]
pub fn text_field(text: &str, field: &Field, kind: &str, hint: Option<&str>) -> Element

#[must_use]
pub fn date_field(text: &str, field: &Field) -> Element

#[must_use]
pub fn time_field(text: &str, field: &Field) -> Element
```

`text_field` is a labelled text input bound to a [`Field`](api-form.md#field), with its hint and error. `kind` is the input `type` (`text`, `email`, `password`, `search`, `tel`, ...). `date_field` is `text_field(text, field, "date", None)` (the platform's date picker, holding `YYYY-MM-DD`; pair with `rules::date`); `time_field` uses `"time"` (`HH:MM`; `rules::time`).

Emits:

```text
div.q-textfield
  label.q-textfield-label[for=field.name]  text
  input.q-textfield-input[type=kind placeholder=hint (if Some)]    <- Field::input(): id, name, value, aria-invalid, aria-describedby, listeners
  p.q-textfield-hint[id="<name>-hint"]  hint                       (if Some)
  p.q-field-error.q-textfield-error[id="<name>-error" role="alert"]   <- Field::error_view()
```

The hint element has an id but nothing references it: `aria-describedby` on the input points only at the error element (and only while there is an error). `hint` is also used as the `placeholder`.

## fab

```rust
#[must_use]
pub fn fab(label: &str, glyph: &str, on_click: impl Fn() + 'static) -> Element
```

The floating button for a screen's main action: `icon_button(...)` with the extra class `q-fab` (so `button.q-iconbtn.q-fab`).

## tabs

```rust
#[must_use]
pub fn tabs(
    id: &str,
    labels: &[&str],
    selected: &Signal<usize>,
    panel: impl Fn(usize) -> View + 'static,
) -> Element
```

Tabs over panels. `id` prefixes the element ids and must be unique on the page. `panel(i)` builds the content of tab `i`; it is rebuilt each time the selection changes (the call is untracked).

Keyboard: ArrowRight and ArrowLeft move to the next / previous tab and wrap; Home goes to the first and End to the last; the default action of those keys is prevented (Home and End would scroll the page). After a key move the newly selected tab takes focus; a click does not move focus programmatically. Other keys are ignored. Roving tabindex: the selected tab has `tabindex="0"`, the others `"-1"`.

Emits:

```text
div.q-tabs
  div.q-tablist[role="tablist"]
    button.q-tab[type="button" role="tab" id="<id>-tab-<i>" aria-controls="<id>-panel"
                 aria-selected="true"|"false" tabindex="0"|"-1"]  label     (+ class q-tab-on when selected)
    ...
  div.q-tabpanel[role="tabpanel" id="<id>-panel" tabindex="0" aria-labelledby="<id>-tab-<selected>"]
    <panel(selected)>
```

Every tab's `aria-controls` is the one panel id. With empty `labels` no tab is built (and no key handler exists). A `selected` out of range selects no tab but still calls `panel(selected)`.

## pull_to_refresh

```rust
pub const PULL_THRESHOLD: f64 = 64.0;

#[must_use]
pub fn pull_to_refresh(
    refreshing: &Signal<bool>,
    on_refresh: impl Fn() + 'static,
    content: View,
) -> Element
```

Pull down at the top of `content` to refresh. `PULL_THRESHOLD` is how far a pull must go, in pixels. `on_refresh` is called when the pull is released at or past the threshold while not already refreshing; at that moment the component sets `refreshing` to true. **The app must set `refreshing` back to false** when done.

Behaviour, from the gesture handler (`Axis::Vertical`, default `Config` otherwise):

* `Drag { dy }` while the scroll position is at the top and not refreshing: the pull distance is `dy` clamped to `0..=96` (`PULL_THRESHOLD * 1.5`).
* `DragEnd`: the distance is reset to 0; if it was at least `PULL_THRESHOLD` and not refreshing, `refreshing` is set to true and `on_refresh()` is called.
* `Cancel`: distance reset to 0.
* "At the top" is tracked from `Scroll` events on the wrapper (`ScrollEvent.top <= 0.0`); it starts true.

Emits:

```text
div.q-ptr.q-touch-pan-x  (+ class q-ptr-top while at the top)   [style custom property --q-pull: "<N>px"]
  div.q-ptr-indicator[role="progressbar" aria-label="Refreshing"
                      aria-hidden="true"|"false"]               (+ class q-ptr-busy while refreshing)
  div.q-ptr-content
    <content>
```

`aria-hidden` is `"true"` while not refreshing and the pull distance is 0, `"false"` otherwise. `--q-pull` is set with the reactive `css` (not an inline `style` attribute), so it works under a strict CSP. `aria-label` is the fixed English text `Refreshing`. Touch only: with a mouse, offer a button. Whether the browser hands the app the downward pull at the top of a scroll area is a matter of `touch-action`, set by the `q-touch-pan-x` class (from `Axis::Vertical`; see [api-platform](api-platform.md#axis)) and the `q-ptr-top` class; check it on the devices you ship to.

## swipe_action

```rust
pub const SWIPE_REVEAL: f64 = 88.0;

#[must_use]
pub fn swipe_action(content: View, action: &str, on_action: impl Fn() + 'static) -> Element
```

A row that slides left to show an action button (delete, archive). The button is always in the page, so the keyboard and screen readers reach it; the swipe is the touch shortcut. `SWIPE_REVEAL` is how far the row slides, in pixels.

Behaviour (gesture axis `Horizontal`):

* `Drag { dx }`: the starting offset is `-88` if the row was already open (offset at most `-87`), else 0; the offset is `(start + dx)` clamped to `-88..=0`.
* `DragEnd`: the row snaps open (`-88`) if past halfway (offset below `-44`), else closed (0).
* `Cancel`: closed.
* The action button sets the offset to 0, then calls `on_action`.

Emits:

```text
div.q-swipe.q-touch-pan-y
  div.q-swipe-back
    button.q-swipe-action[type="button"]  action
  div.q-swipe-front[style custom: transform: translateX(<offset>px)]     (reactive `css`)
    <content>
```

Typical use: `swipe_action(tile("Task", None, None, None, None).into_view(), "Delete", move || remove(id))` inside a `li` that goes into `list`.
