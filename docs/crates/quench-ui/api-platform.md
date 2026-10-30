---
title: Platform, animation, gestures
parent: quench-ui
nav_order: 6
---

Reference of three modules: `quench_ui::platform` (`src/platform.rs`: clock, timers, motion preference), `quench_ui::anim` (`src/anim.rs`: `presence`) and `quench_ui::gesture` (`src/gesture.rs`: taps, drags, swipes). The [component kit](api-kit.md) is built on all three.

All state is thread-local or `Rc`-based: nothing here is `Send` or `Sync` (`dyn Platform` is installed per thread).

## platform

What the UI needs from the host beyond drawing: a clock, one-shot timers and the user's motion preference. The browser backend installs its own (`WebPlatform` in `quench-ui-web`), the native backend has `NativePlatform` (`quench-ui-native`), and `ManualPlatform` is one a test drives by hand.

```rust
use quench_ui::platform::{self, ManualPlatform, Platform, Timer, after, install, installed, now, prefers_reduced_motion};
```

### Platform

```rust
pub trait Platform {
    /// Milliseconds on a monotonic clock.
    fn now(&self) -> f64;
    /// Runs `f` once after `ms` milliseconds, unless cancelled. Returns an id for [`Platform::cancel`].
    fn after(&self, ms: u32, f: Box<dyn FnOnce()>) -> u32;
    fn cancel(&self, id: u32);
    /// Whether the user asked for less motion (`prefers-reduced-motion: reduce`).
    fn prefers_reduced_motion(&self) -> bool { false }
}
```

The services a host provides. `prefers_reduced_motion` has a default of `false`. Ids are the implementation's; `Timer` stores them as `u32`.

### Functions

| Function | Signature | Behaviour |
|---|---|---|
| `install` | `pub fn install(platform: Rc<dyn Platform>)` | Makes `platform` the one used **on this thread**, replacing any earlier one. |
| `installed` | `#[must_use] pub fn installed() -> bool` | Whether a platform is installed on this thread. |
| `now` | `#[must_use] pub fn now() -> f64` | The platform's clock, or `0.0` when none is installed. |
| `after` | `pub fn after(ms: u32, f: impl FnOnce() + 'static) -> Timer` | Runs `f` after `ms` ms. **With no platform installed it runs `f` at once** (synchronously, before returning) and returns `Timer(0)`, so a UI built without a host (a server render) still reaches its final state. |
| `prefers_reduced_motion` | `#[must_use] pub fn prefers_reduced_motion() -> bool` | `false` with no platform installed. |

### Timer

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timer(u32);
impl Timer { pub fn cancel(self) }
```

A timer that can be cancelled. **Dropping it does not cancel it.** `cancel` asks the installed platform to cancel the id; with none installed it does nothing. Cancelling a timer that already ran or was cancelled is harmless for `ManualPlatform` (it removes nothing).

### ManualPlatform

```rust
pub struct ManualPlatform { /* private */ }
impl Default for ManualPlatform
impl Platform for ManualPlatform
impl Debug for ManualPlatform   // now, reduced, pending timer count
```

A platform whose clock and timers move only when told to; for tests.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub const fn new() -> Self` | Clock at 0.0, no timers, motion not reduced. Ids start at 1. |
| `advance` | `pub fn advance(&self, ms: f64)` | Moves the clock forward by `ms`, running the timers that fall due, earliest first (a timer that a callback starts and that falls due within the same span runs too, in the same call). While a callback runs the clock reads the timer's due time (never earlier than before); at the end it reads `start + ms`. |
| `pending` | `#[must_use] pub fn pending(&self) -> usize` | Timers that have not run. |
| `set_reduced_motion` | `pub fn set_reduced_motion(&self, on: bool)` | What `prefers_reduced_motion` answers. |

Trait behaviour: `after` stores `(id, now + ms)` and returns the next id; `cancel` removes by id; timers with equal due times run in a stable order of insertion by `min_by` (the first stored among equals). Callbacks run with no internal borrow held, so they may call `after` and `cancel`.

```rust
use std::rc::Rc;
use quench_ui::platform::{self, ManualPlatform};

let manual = Rc::new(ManualPlatform::new());
platform::install(manual.clone());
let t = platform::after(100, || println!("late"));
assert_eq!(manual.pending(), 1);
manual.advance(99.0);          // nothing yet
manual.advance(1.0);           // prints "late"
assert_eq!(manual.pending(), 0);
let _ = t;
```

## anim

Animation helpers that lean on CSS transitions: the app says when something is shown, CSS says how it moves. Everything honours the user's reduced-motion preference by skipping the wait.

### presence

```rust
#[must_use]
pub fn presence(
    visible: impl Fn() -> bool + 'static,
    duration_ms: u32,
    content: impl Fn() -> View + 'static,
) -> View
```

Content that fades (or slides, or scales) in when `visible` becomes true and stays in the page until its exit has played when it becomes false. `visible` is read in an effect (so it may read signals); `content` is called, untracked, each time the content is built; `duration_ms` is the length of the CSS transition, used for the fallback timer.

The result is built lazily (`View::lazy`): the effect and timers are created when the view is mounted, under the mount's scope, and the pending timer is cancelled by `on_cleanup` when that scope is disposed.

Emitted markup, only while the content is present:

```text
div.q-anim[.q-anim-in]
  <content()>
```

Nothing (an empty region) while absent. Style the two states and a CSS `transition` does the rest, for example:

```css
.q-anim { opacity: 0; transition: opacity 200ms; }
.q-anim.q-anim-in { opacity: 1; }
```

**State machine** (internal phases `Gone`, `Entering`, `In`, `Leaving`; the effect re-runs when `visible` changes):

| `visible` becomes | From | Action |
|---|---|---|
| true | `Gone`, reduced motion | to `In` at once (class `q-anim-in` present on build). |
| true | `Gone` | to `Entering` (element added **without** `q-anim-in`); a 20 ms timer (`ENTER_DELAY_MS`) then moves to `In` (class added), so the browser has drawn the first state and has something to transition from. |
| true | `Leaving` | cancel the leave timer, back to `In`. The element stays; its content is not rebuilt. |
| false | `Entering` or `In`, reduced motion | cancel timers, to `Gone` at once. |
| false | `Entering` or `In` | to `Leaving` (class removed); a timer of `duration_ms + 50` ms (`LEAVE_SLACK_MS`) then moves to `Gone`. |
| any other combination | | nothing. |

The leaving element also goes to `Gone` as soon as it receives a `transitionend` event (the listener is on the `div.q-anim` wrapper), without waiting for the timer. The timer is the fallback for when `transitionend` never comes (the element was hidden, the transition was cancelled). The region is rebuilt only when presence changes (`Gone` or not), not at every phase.

A `visible` that is true when the view is built starts in `In` with no animation, so a server render and the first client render agree. With no platform installed the timers run at once, so an entering element reaches `In` and a leaving one `Gone` immediately.

Used by `kit::sheet`, `kit::dialog` and `kit::snackbar`.

```rust
use quench_ui::anim::presence;
use quench_ui::prelude::*;

let open = create_signal(false);
let shown = open.clone();
let view = presence(move || shown.get(), 200, || p().text("hello").into_view());
```

## gesture

Taps, long presses, drags and swipes from pointer events. `Recognizer` is a plain state machine (feed it positions and times, get gestures back, test it without a browser); `on_gesture` wires one to an element.

```rust
use quench_ui::gesture::{Axis, Config, Direction, Gesture, Recognizer, on_gesture};
```

### Direction

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction { Left, Right, Up, Down }
```

### Gesture

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gesture {
    Tap,                                              // pressed and released in place, not a long press
    LongPress,                                        // held in place for the long-press time
    DragStart,                                        // moved beyond the slop
    Drag { dx: f64, dy: f64 },                        // relative to where the drag started
    DragEnd { dx: f64, dy: f64, vx: f64, vy: f64 },   // velocity in px per ms
    Swipe(Direction),                                 // a drag that ended fast and far enough
    Cancel,                                           // the browser took the pointer over, or abandoned
}
```

### Axis

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis { Both, Horizontal, Vertical }
```

The directions the browser may use for its own scrolling and zooming (`touch-action`). It is set through a class, because a strict CSP forbids inline styles; the stylesheet has to define the classes. A touch that the browser takes over to scroll arrives as `pointercancel`.

| Variant | Meaning | Class added by `on_gesture` | Intended `touch-action` |
|---|---|---|---|
| `Both` | The app takes every direction; the browser does nothing here. | `q-touch-none` | `none` |
| `Horizontal` | The app takes left and right; vertical scrolling stays with the browser. | `q-touch-pan-y` | `pan-y` |
| `Vertical` | The app takes up and down; horizontal scrolling stays with the browser. | `q-touch-pan-x` | `pan-x` |

The CSS mapping in the last column is the meaning of the class names; the classes themselves are defined by the style crate, not here.

### Config

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    pub slop: f64,            // px a press may move and still be a tap / long press
    pub long_press_ms: f64,
    pub swipe_distance: f64,  // least distance of a swipe, px
    pub swipe_speed: f64,     // least speed of a swipe, px per ms
    pub axis: Axis,
}
```

`Default`:

| Field | Default |
|---|---|
| `slop` | `10.0` |
| `long_press_ms` | `500.0` |
| `swipe_distance` | `40.0` |
| `swipe_speed` | `0.3` |
| `axis` | `Axis::Both` |

### Recognizer

```rust
#[derive(Debug)]
pub struct Recognizer { /* private */ }
```

The state machine behind `on_gesture`. States: idle, down, dragging. Times are in milliseconds on any monotonic clock.

| Method | Signature | Behaviour |
|---|---|---|
| `new` | `#[must_use] pub const fn new(config: Config) -> Self` | Idle. |
| `down` | `pub const fn down(&mut self, x: f64, y: f64, t: f64)` | Pointer pressed at `(x, y)`; state becomes *down* (replaces any earlier state without emitting). |
| `moved` | `pub fn moved(&mut self, x: f64, y: f64, t: f64) -> Vec<Gesture>` | Idle: nothing. Down: nothing while the distance from the start (Euclidean, `hypot`) is at most `slop`; beyond it, becomes *dragging* and returns `[DragStart, Drag{dx, dy}]`. Dragging: returns `[Drag{dx, dy}]` with offsets from the start; keeps the previous sample for the end velocity unless `t` is not later than the last sample. |
| `up` | `pub fn up(&mut self, x: f64, y: f64, t: f64) -> Vec<Gesture>` | Back to idle. Idle: nothing. Down: `[Tap]`, or nothing if a long press already fired. Dragging: `[DragEnd{dx, dy, vx, vy}]` and, if the flick test passes, `Swipe(direction)` after it. |
| `long_press_due` | `pub fn long_press_due(&mut self, t: f64) -> Option<Gesture>` | If down, no long press yet, and `t - start >= long_press_ms`: marks it and returns `Some(LongPress)`. Else `None`. |
| `cancel` | `pub fn cancel(&mut self) -> Vec<Gesture>` | Back to idle. Returns `[Cancel]` only if it was dragging; nothing for down or idle. |

Velocity at `up`: the span is `max(t - before.t, t - last.t, 1.0)` ms, where `before` is the sample before the last move; `vx = (x - before.x) / span`, `vy` likewise (px/ms). The 1 ms floor avoids division by zero.

Swipe test: horizontal if `|dx| >= |dy|`; the distance and speed are those on that axis; a swipe needs `distance >= swipe_distance` and `speed >= swipe_speed`. The direction is `Right`/`Left` by the sign of `dx` or `Down`/`Up` by the sign of `dy` (positive `dy` is down, zero counts as negative: `Left`/`Up`).

A long press does not prevent the following drag: moving beyond the slop after a long press still starts a drag. Releasing in place after a long press emits no `Tap`.

### on_gesture

```rust
#[must_use]
pub fn on_gesture(element: Element, config: Config, handler: impl Fn(&Gesture) + 'static) -> Element
```

Attaches gesture recognition to `element`: `handler` receives each `Gesture` as it happens. Adds the class of `config.axis` and listeners for `PointerDown`, `PointerMove`, `PointerUp` and `PointerCancel` using the `x`, `y` of the `PointerEvent` and `platform::now()` for time.

* `PointerDown`: cancels a running long-press timer, feeds `down`, starts a timer of `long_press_ms` (clamped to `0..=u32::MAX` ms) that asks the recognizer `long_press_due(platform::now())` and emits the result. The timer is `platform::after`, so with no platform installed it runs at once; the recognizer's clock is `platform::now()` (0), so `long_press_due` returns `LongPress` immediately only when `long_press_ms` is 0 or less.
* `PointerMove`: feeds `moved`; the long-press timer is cancelled as soon as the move produces any gesture (that is, once the slop is exceeded).
* `PointerUp`: cancels the timer, feeds `up`.
* `PointerCancel`: cancels the timer, feeds `cancel`.

Handlers are called with no recognizer borrow held, so a handler may do anything, including starting another gesture. All gestures produced by one event are delivered in order. Multi-touch is not tracked: events from any pointer feed the single recognizer.

```rust
use quench_ui::gesture::{Config, Gesture, on_gesture};
use quench_ui::prelude::*;

let el = on_gesture(div().text("tap me"), Config::default(), |g| {
    if matches!(g, Gesture::Tap) { /* ... */ }
});
```

The `kit` components `pull_to_refresh` and `swipe_action` are built with `on_gesture` ([api-kit](api-kit.md)).
