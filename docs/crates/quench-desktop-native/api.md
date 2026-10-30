---
title: API reference
parent: quench-desktop-native
grand_parent: Crates
nav_order: 2
---

Everything is at the crate root (`src/lib.rs`). Linux only.

## `key_event`

```rust
#[must_use]
pub fn key_event(key: &Key, m: Modifiers) -> KeyEvent
```

The DOM's view of a key press: `KeyEvent { key: key.name.clone(), ctrl, shift, alt, meta }` from `m`. (`caps` and `altgr` are not carried.)

## `struct Controller`

```rust
pub struct Controller { /* renderer, held key, caret blink, size, pointer, closing, ime, repeat */ }
```

Turns window events into calls on a renderer. It owns no window, so it can be driven by any source of `WindowEvent`s (and by a test). `Debug` prints `size` and `closing`.

### `Controller::new`

```rust
#[must_use]
pub fn new(renderer: &Rc<NativeRenderer>, size: (usize, usize)) -> Self
```

Calls `renderer.set_viewport(width, height)`. Key repeat starts at `(25, 600)`.

### `Controller::closing`

```rust
#[must_use]
pub const fn closing(&self) -> bool
```

Whether the user asked for the window to close.

### `Controller::size`

```rust
#[must_use]
pub const fn size(&self) -> (usize, usize)
```

The size the window was last told to be.

### `Controller::ime_change`

```rust
pub fn ime_change(&mut self) -> Option<Option<ImeState>>
```

The text-field state the input method should be told about now, if it differs from what it was told: `None` = nothing to tell; `Some(None)` = no field has focus any more; `Some(Some(state))` = a field, its text or its caret changed.

### `Controller::ime_enabled`

```rust
#[must_use]
pub const fn ime_enabled(&self) -> bool
```

Whether the input method was enabled by the last `ime_change` (so a change is an update, not an enable).

### `Controller::set_repeat`

```rust
pub const fn set_repeat(&mut self, repeat: (i32, i32))
```

Sets key repeat: characters per second and the delay before it starts in milliseconds.

### `Controller::next_timer`

```rust
#[must_use]
pub fn next_timer(&self) -> Instant
```

When the next timer (caret blink, key repeat) is due.

### `Controller::timers`

```rust
pub fn timers(&mut self, now: Instant)
```

Runs the due timers: toggles the caret every 530 ms (`set_caret_visible` only while a text field has focus); repeats the held key at `1 / max(rate, 1)` seconds intervals.

### `Controller::handle`

```rust
pub fn handle(&mut self, event: WindowEvent)
```

Mapping:

| Event | Effect on the renderer |
|---|---|
| `CloseRequested` | `closing()` becomes true. |
| `Resized` | stores the size; `set_viewport(width, height)`. |
| `PointerEntered`, `PointerMoved` | `pointer_move(x, y)`; remembers the position. |
| `PointerLeft` | `pointer_move(-1, -1)`. |
| `PointerButton` with `button == 0x110` | `pointer_down(x, y)` / `pointer_up(x, y)`. Other buttons are ignored. |
| `Scroll { axis, value }` | `wheel(px, py, dx, dy)` at the last pointer position with `value * 4.0` pixels; axis 0 is `dy`, otherwise `dx`. |
| `Key` of a modifier key | ignored. |
| `Key` pressed | `key_down(key_event)`; if the key has text and no ctrl, alt or meta is held, also `text(text)`. Starts repeating if the key has text or is Backspace, Delete or an arrow, after the repeat delay. |
| `Key` released | stops repeating if it is the held key; `key_up`. |
| `Ime(change)` | `ime_apply(delete_before, delete_after, commit, preedit as (text, cursor_begin))`. |
| `Frame`, `PointerButton` of other buttons, others | ignored. |

## `struct Host`

```rust
pub struct Host { /* window, controller, optional accessibility service, hurry deadline */ }
```

A window and the renderer it shows. `Debug` prints `size` and `closing`.

### `Host::open`

```rust
pub fn open(title: &str, app_id: &str, size: (usize, usize), renderer: &Rc<NativeRenderer>) -> io::Result<Self>
```

Opens the window (`Window::open`), starts the accessibility service (`Service::start(title)`; failure is ignored and the window runs without accessibility), creates a `Controller`, draws the first frame and publishes the tree. Errors: no Wayland compositor, or one lacking what a window needs.

### `Host::closing`

```rust
#[must_use]
pub const fn closing(&self) -> bool
```

### `Host::has_accessibility`

```rust
#[must_use]
pub const fn has_accessibility(&self) -> bool
```

Whether the accessibility service started (the bus was there and answered). It does not say that a screen reader is attached.

### `Host::draw`

```rust
pub fn draw(&mut self) -> io::Result<bool>
```

Draws a frame if the renderer reports a change (`take_dirty`): `begin_frame`, `render`, `present`. Returns whether it drew.

### `Host::step`

```rust
pub fn step(&mut self, timeout: Duration) -> io::Result<()>
```

One iteration: waits up to the least of `timeout`, the renderer's `next_wake`, the controller's next timer and (while a screen reader asked within three seconds) 25 ms; takes the first event, sets the controller's key repeat from the window, delivers it and then every event that arrives within 1 ms of the previous; runs timers and `renderer.tick()`; carries out accessibility actions and edits; draws (publishing the tree if it drew); tells the input method about a changed field. Clipboard: before delivering Ctrl+V (key name `v`/`V`, pressed) the system clipboard text is read and given to the renderer (`set_clipboard`); after every event, text the renderer wants copied (`take_clipboard_out`) is written to the system clipboard. Errors: the connection's.

## `run`

```rust
pub fn run(title: &str, app_id: &str, size: (usize, usize), renderer: &Rc<NativeRenderer>) -> io::Result<()>
```

`Host::open`, then `step(1 s)` until `closing()`.
