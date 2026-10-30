---
title: Timers API
parent: quench-ui-native
nav_order: 7
---

Source: `src/timers.rs`. Time for the views: the clock and timers that `quench_ui::platform` asks for (toasts that go away, delays, debounces), and a way for other threads to hand results to the views. Everything runs on the thread of the renderer, inside the host's `tick`.

## The platform

`NativeRenderer::new()` installs a private `NativePlatform` as `quench_ui::platform::Platform` for the thread:

| Method | Behaviour |
|---|---|
| `now() -> f64` | Milliseconds since the renderer was created (monotonic, `Instant`). |
| `after(ms: u32, f: Box<dyn FnOnce()>) -> u32` | Registers a one-shot timer due `ms` ms from now; returns an id that starts at 1 and increases. |
| `cancel(id: u32)` | Removes the timer; unknown ids are ignored. |
| `prefers_reduced_motion() -> bool` | The `reduced_motion` of the last `set_media` (false initially). |

User code reaches it through `quench_ui::platform::{after, now, prefers_reduced_motion}` (and the returned cancel handle), not through this crate.

## tick

```rust
pub fn tick(&self) -> bool
```

Runs the timers that are due (oldest due time first, repeatedly until none is due; a callback may register more), calls every channel poller (draining all pending messages and running their handlers in order, with no renderer borrow held, so handlers may change views and open channels), and counts a running animation as activity. Returns `true` when any timer ran, any message was handled, or an animation is active; in that case the document is marked dirty with a restyle and stale (the host should `take_dirty` and render). Call it every time the host wakes, and at least once after `next_wake` elapsed.

## next_wake

```rust
#[must_use]
pub fn next_wake(&self) -> Option<std::time::Duration>
```

How long until the host should wake: the time to the earliest timer (saturating at zero), capped at 16 ms while an animation runs, capped at 25 ms while any channel exists (channels are polled, there is no wake-up from other threads). `None` when nothing will happen without input.

## advance_animations

```rust
pub fn advance_animations(&self, by: std::time::Duration)
```

Moves the clock the animations use forward by `by` (adds to a skew). For tests and for a still at a chosen moment of a transition or animation. It does not move the timer clock and does not mark the document dirty; the next layout sees the new time.

## channel

```rust
pub fn channel<T: Send + 'static>(&self, handler: impl Fn(T) + 'static) -> std::sync::mpsc::Sender<T>
```

A channel into the views from other threads: what is sent (from any thread, through the `Sender`) is handled on the renderer's thread, in order, at the next `tick`. The handler need not be `Send`. Dropping the `Sender` does not unregister the poller; a closed channel is simply empty. There is no way to remove a channel (the poll interval stays 25 ms for the life of the renderer).

```rust
let tx = renderer.channel(move |catalog: Vec<Row>| rows.set(catalog));
std::thread::spawn(move || { let _ = tx.send(fetch()); });
```
