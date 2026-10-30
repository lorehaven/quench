---
title: Input API
parent: quench-ui-native
nav_order: 5
---

Source: `src/input.rs`. The host calls these methods with what the window saw; the renderer finds the target in the last layout, delivers `quench_ui::Event`s through the DOM and does what a browser does unless a listener prevented it. All are methods of `NativeRenderer`; all coordinates are in window (device) pixels, the space of `set_viewport`.

## ImeState

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct ImeState {
    pub text: String,                  // the focused field's text
    pub cursor: usize,                 // caret, byte offset in `text`
    pub anchor: usize,                 // other end of the selection, byte offset
    pub rect: (f32, f32, f32, f32),    // caret rectangle in the window: (x, y, 1.0, height)
}
```

Note the unit: byte offsets here, characters in [`AccessNode::caret`](api-access.md#accessnode).

## Events and default actions

`dispatch(target, event, bubbles)` (private) collects the handlers of the target and, when bubbling, of every ancestor up to `<html>` (target first); handlers registered with `listen` for the event's kind run in registration order. Before delivery the thread's prevent request is cleared; after it, it is read: if a listener called `quench_ui::event::prevent_default()` the default action is skipped (or `key_down` returns `true`). The document is marked dirty with a restyle after every dispatch except `Scroll` (paint only).

Which events are delivered, to what, and whether they bubble:

| Event | Delivered to | Bubbles | By |
|---|---|---|---|
| `PointerMove(PointerEvent)` | hit element | yes | `pointer_move`, `pointer_down` (through `pointer_move`) |
| `PointerDown` | hit element | yes | `pointer_down` |
| `PointerUp` | hit element (or the pressed one for `pointer_release`) | yes | `pointer_up`, `pointer_release` |
| `PointerCancel` | the element the press began on | yes | `pointer_cancel` (position `-1,-1`) |
| `Click` | nearest common ancestor of press and release targets | yes | `pointer_up`, Enter/Space on a button or link, Space on a checkbox/radio, label forwarding, `perform(Click)` |
| `Focus`, `Blur` | the node gaining / losing focus | no | any focus change |
| `KeyDown(KeyEvent)`, `KeyUp` | focused element, else `<body>` | yes | `key_down`, `key_up` |
| `Input(String)` | the field or slider | yes | edits, `text`, `set_field_value`, sliders (value after the edit) |
| `Submit` | the `<form>` | no | Enter in a text field; click on a submit button |
| `Scroll(ScrollEvent{top,left})` | the scrolled box | no | wheel and scroll keys when the position changed |
| `CompositionStart(String)`, `CompositionEnd(String)` | the field | yes | `ime_apply` (value when composing began / ended) |

`pointer_id` of every `PointerEvent` is 1. Events never produced here: `DoubleClick`, `Change`, `TransitionEnd`, `Resize` (see [internals](internals.md#events-not-produced)).

Hit testing (`hit`, `element_at`): walks the last layout from the root. Boxes with non-visible `overflow` clip the hit area to their padding box; scroll offsets of clipping boxes (and the root, key 0) shift their children; `position: fixed` children ignore the scrolling of what they are in; `transform: translate` offsets a box. Order at each box: layers above (positive z), inline-level children, text and inline fragments from last to first, block children, layers below, then the box itself. Text fragments count (a click on words lands on their `<span>`); a text node's hit is its parent element; text of a field belongs to the field. Boxes with key `usize::MAX` (the anonymous centring block) never match.

## State and queries

### take_dirty

```rust
pub fn take_dirty(&self) -> bool
```

Returns whether anything changed since the last call and clears the flag. The host redraws when it is `true`.

### set_caret_visible

```rust
pub fn set_caret_visible(&self, on: bool)
```

Shows or hides the caret of the focused field (the host blinks it). Sets dirty but not restyle, so the next frame reuses the layout.

### scroll_of

```rust
#[must_use]
pub fn scroll_of(&self, key: usize) -> (f32, f32)
```

Scroll position `(x, y)` of the scroll container with box key `key`; `(0, 0)` if never scrolled. The document itself is key 0 (`<html>`).

### focused

```rust
#[must_use]
pub fn focused(&self) -> Option<NodeId>
```

### text_field_focused

```rust
#[must_use]
pub fn text_field_focused(&self) -> bool
```

`true` when the focused node is a text field (so the host blinks a caret / shows a keyboard); a focused button is not.

### element_at

```rust
#[must_use]
pub fn element_at(&self, x: f32, y: f32) -> Option<NodeId>
```

The deepest element a pointer at `(x, y)` would land on. Lays out first if the document changed.

### touch_action_at

```rust
#[must_use]
pub fn touch_action_at(&self, x: f32, y: f32) -> quench_css::TouchAction
```

The directions of finger movement the platform may take for itself for a touch beginning at `(x, y)`: the intersection (`TouchAction::and`) of `touch-action` of the element there and of every ancestor. `TouchAction::AUTO` for a touch on nothing. A page that listens to a swipe sets `touch-action` on it; the host then routes drags to `pointer_drag`/`pointer_release` instead of scrolling.

## Pointer

### pointer_move

```rust
pub fn pointer_move(&self, x: f32, y: f32)
```

Updates the hovered element (restyles on change: `:hover`), moves a slider that is being pressed, and delivers `PointerMove` to the element under the pointer (bubbling).

### pointer_down

```rust
pub fn pointer_down(&self, x: f32, y: f32)
```

Primary button down. Calls `pointer_move` first. On nothing: blur. Else records the pressed and active element, delivers `PointerDown`; unless prevented: focus goes to the nearest focusable ancestor-or-self (mouse focus: `:focus-visible` only for text fields; blur if there is none); on a text field the caret goes to the character nearest `x`; on a range input the value follows `x`.

### pointer_up

```rust
pub fn pointer_up(&self, x: f32, y: f32)
```

Primary button up. Clears active and pressed; delivers `PointerUp` to the element under the pointer (nothing there: returns). If something was pressed, delivers a click to the common ancestor of the pressed and released elements, see [Click behaviour](#click-behaviour). Releasing on a different button therefore produces no click on either (their common ancestor is not a control).

### pointer_cancel

```rust
pub fn pointer_cancel(&self)
```

The gesture is not a click after all (a touch turned into a scroll). Clears pressed and active; delivers `PointerCancel` to the element the press began on. Nothing is clicked.

### pointer_drag

```rust
pub fn pointer_drag(&self, x: f32, y: f32)
```

A finger the page owns (`touch_action_at`) moved: `PointerMove` goes to the element the touch began on, wherever the finger is.

### pointer_release

```rust
pub fn pointer_release(&self, x: f32, y: f32)
```

A finger the page owns came up: `PointerUp` goes to the element the touch began on; clears active/pressed; no click.

### wheel

```rust
pub fn wheel(&self, x: f32, y: f32, dx: f32, dy: f32)
```

Scrolls the nearest scroll container at or above the element at `(x, y)` that can still move by `(dx, dy)` pixels (`dy` positive = down). A container scrolls on an axis if `overflow` is `auto` or `scroll`; the maximum is content overflow minus padding size. The document (key 0) scrolls vertically by `max(content height, box height) - viewport height`, never horizontally. Positions are clamped to `[0, max]`; the first container whose position changes takes the scroll, a `Scroll` event is delivered to it (no bubbling) and the frame is marked paint-only dirty.

### Click behaviour

`click(node)` (private), from a release, from an activation key or from `perform(Click)`:

1. If `node` is inside a non-disabled `input[type=checkbox|radio]` (nearest ancestor-or-self), it is toggled: a checkbox flips the `checked` attribute; a radio clears `checked` on every radio of the document with the same `name` (document-wide, not form-scoped; no `name`: only itself) and sets it.
2. `Click` is delivered (bubbling).
3. If not prevented and `node` is within a `<label>` (and not within a focusable element before reaching it): the label's control (`for` = element `id`; else the first focusable descendant in document order) is focused and clicked.
4. If not prevented and `node` is within a submit button (`<button>` whose `type` is missing, `submit` or empty; or `<input type=submit>`) inside a `<form>`: `Submit` is delivered to the form (no bubbling). A button of another type stops the search.

### Focus

`focus_node(id, keyboard)` (also behind `Renderer::focus`): no-op if already focused; delivers `Blur` to the previous node, sets focus, sets `:focus-visible` when `keyboard` or the node is a text field, puts the caret and anchor at the end of the field's value, delivers `Focus` (no bubbling).

Focusable elements (`is_focusable`): not disabled, and: a `tabindex` that parses as an integer `>= 0` (negative or non-numeric: not focusable); else `button`, `select`, `textarea`; `input` unless `type=hidden`; `a` with an `href`. Others are not focusable. Tab order is document order (no positive-`tabindex` reordering); elements without a box in the last layout (`display: none`) are skipped.

## Keyboard

### key_down

```rust
pub fn key_down(&self, key: &quench_ui::KeyEvent) -> bool
```

Returns whether a listener took the key. Order:

1. `KeyDown` is delivered to the focused element (else `<body>`), bubbling. If a listener prevented it: return `true`, nothing else happens.
2. `Tab` / `Shift+Tab`: focus the next/previous focusable element, cycling, inside `trap_focus` scope if any (whole document otherwise); from no focus: first (Tab) or last (Shift+Tab). Focus is keyboard focus.
3. No focused element: scroll keys (below).
4. Focused text field: editing keys (below).
5. Focused `input[type=range]`: slider keys (below).
6. `Enter` or `Space` (`" "`) on a `button` or `a`; `Space` on a checkbox/radio input: a click on the element.
7. Anything else: scroll keys.

Returns `false` in cases 2 to 7.

**Scroll keys** (`scroll_key`): `ArrowDown/Up` 40 px vertically, `ArrowRight/Left` 40 px horizontally, `PageDown` or `" "` 0.9 x viewport height down, `PageUp` 0.9 x up, `Home`/`End` 1e6 px up/down (to the top/bottom). They scroll from the hovered element, else the focused one, else `<body>`, with the same rules as `wheel`. (The `ctrl` argument is ignored.)

**Editing keys** (`edit_key`, for `input` of types `text`, `email`, `password`, `search`, `tel`, `url`, `number`, and `textarea`):

| Key | Effect |
|---|---|
| `ArrowLeft`, `ArrowRight` | Collapses a non-empty selection to its start/end (without Shift); otherwise moves the caret one character; with Ctrl one word (whitespace-delimited); with Shift extends the selection |
| `Home`, `End` | Caret to start / end (Shift extends) |
| `Backspace`, `Delete` | Deletes the selection; else one character before/after the caret, or with Ctrl one word before/after; nothing at the ends |
| `Enter` | in a `textarea`: inserts `"\n"`; otherwise delivers `Submit` to the enclosing `<form>` (nothing without one) |
| Ctrl+`a`/`A` | Selects all |
| Ctrl+`c`/`C` | Copies the selection to the internal clipboard and flags it for the host |
| Ctrl+`x`/`X` | Copy, then delete the selection |
| Ctrl+`v`/`V` | Inserts the internal clipboard text through `text` |
| others | ignored (typed characters arrive through `text`) |

Every change to the value sets the `value` attribute, moves caret and anchor, and delivers `Input(new value)` (bubbling).

**Slider keys** (`range_key`): `ArrowRight`/`ArrowUp` +step, `ArrowLeft`/`ArrowDown` -step, `PageUp` +big, `PageDown` -big, `Home` min, `End` max, where step = `step` attribute if > 0 else 1, big = `max((max - min) / 10, step)`. Other keys fall through to rule 6/7. Min/max/value: see [range_of](api-access.md#range-and-value-of-sliders-and-progress-bars). The value is clamped, snapped to `min + n*step`, written to `value` as a whole number (`"5"`) or up to four decimals without trailing zeros, and `Input` is delivered only if the text changed. Pointer press/drag does the same with the value proportional to `x` across the box (min at the left edge). A slider with `step` missing uses step 1 when snapping.

### key_up

```rust
pub fn key_up(&self, key: &quench_ui::KeyEvent)
```

Delivers `KeyUp` to the focused element (else `<body>`), bubbling. No default action.

### text

```rust
pub fn text(&self, text: &str)
```

Text typed, pasted or committed into the focused text field: replaces the selection. Empty input or no text field focused: ignored. In a single-line input, control characters (including `\n`) are removed; a `textarea` keeps everything. Delivers `Input`.

### set_field_value

```rust
pub fn set_field_value(&self, id: usize, text: &str) -> bool
```

What an assistive technology asks for: if `id` (a raw DOM index) is a text field, focuses it (keyboard focus), selects all, replaces the text with `text` (control characters are *not* filtered here), delivers `Input`, returns `true`. Otherwise `false`.

## Clipboard

### set_clipboard

```rust
pub fn set_clipboard(&self, text: &str)
```

Makes `text` what a paste inserts; the host calls it with the system clipboard before sending a Ctrl+V. Clears the "to be exported" flag.

### take_clipboard_out

```rust
pub fn take_clipboard_out(&self) -> Option<String>
```

The text copied or cut since the last call (once), for the host to put on the system clipboard; `None` if nothing was copied. Ctrl+C with an empty selection copies the empty string and still flags it (returns `Some("")`).

## Text selection by touch

### select_word_at

```rust
pub fn select_word_at(&self, x: f32, y: f32) -> bool
```

A long press: if a text field is at the point, focuses it, places the caret at `x`, selects the word under or touching the caret (alphanumerics and `_`; else one character beside the caret) and returns `true`; `false` if there is no text field there.

### selection_handles

```rust
#[must_use]
pub fn selection_handles(&self) -> Option<[(f32, f32, f32); 2]>
```

For the focused text field with a non-empty selection: `(x, top, height)` in the window of the selection start and end. `None` otherwise.

### move_selection_end

```rust
pub fn move_selection_end(&self, end: bool, x: f32)
```

A handle being dragged: moves one end of the selection to the character nearest `x`. `end == true` means the later end of the selection, else the earlier; the end that moves stays the dragged one, so a handle can cross the other. No effect without a focused text field.

## Input methods

### ime_state

```rust
#[must_use]
pub fn ime_state(&self) -> Option<ImeState>
```

`Some` while a text field is focused and a layout exists: value, caret/anchor as byte offsets, and the caret rectangle `(x, y, 1.0, height)` in the window (`(0, 0, 1, 16)` fallback if the caret cannot be located). During a composition the caret rectangle follows the pre-edit cursor.

### ime_apply

```rust
pub fn ime_apply(
    &self,
    delete_before: u32,
    delete_after: u32,
    commit: Option<&str>,
    preedit: Option<(&str, i32)>,
)
```

One change from an input method, applied in this order:

1. `delete_before`/`delete_after` (bytes) around the caret: the range becomes the selection (counted back/forward in whole characters until the byte count is reached).
2. A non-empty `commit` clears the composition and replaces the selection with it (`Input` delivered). Without a commit, if a deletion was requested and the selection is non-empty it is deleted.
3. `preedit = Some((text, cursor))` with non-empty text shows the composition: delivers `CompositionStart(value)` if none was active; `cursor` is a byte offset into `text` (negative: end). The composition is drawn in the field underlined and is *not* in the value until committed. Any other `preedit` (None or empty text) clears the composition and, if one was active, delivers `CompositionEnd(value)`.

No effect when no text field is focused.
