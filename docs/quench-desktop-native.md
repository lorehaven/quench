# Quench Desktop Native

`quench-desktop-native` (crate `quench_desktop_native`) runs a [`NativeRenderer`](./quench-ui-native.md) in a Wayland window: the host of slice 3.

```rust
let renderer = Rc::new(NativeRenderer::new());
// renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
quench_desktop_native::run("My app", "dev.example.app", (800, 600), &renderer)?;
```

`Host` owns the [`Window`](./quench-desktop-wayland.md) and a **`Controller`**, which turns window events into renderer calls and has no window of its own, so a test drives it with `WindowEvent`s: pointer moves, the primary button (down and up make a click), the wheel (about 10 per notch, 4 pixels each), keys (a key goes to `key_down`; a single character without Ctrl, Alt or Meta is also typed), the **key repeat** the compositor configured (the compositor leaves repeating to the program: a held key repeats after the delay at the rate), the **caret blink** (530 ms), resizes, and input-method changes. `Host::step` waits for an event or the next timer, handles what arrived, draws a frame **only when the renderer says something changed**, and tells the **input method** (text-input-v3) when a text field gets focus, changes or loses it, with the caret's rectangle for placing a candidate list. If the accessibility bus is there, it also publishes the [accessibility tree](./quench-desktop-atspi.md) after each frame and carries out what a screen reader asks for (polling every 25 ms for three seconds after a question, so an answer is quick).

**Clipboard**: before a Ctrl+V the host reads the system clipboard (`Window::clipboard`) into the renderer; after a Ctrl+C or Ctrl+X it puts what was copied on it (`take_clipboard_out` → `Window::set_clipboard`), so text moves between this window and other programs. **Timers**: the wait is also bounded by the renderer's `next_wake()` and `tick()` runs each step (toasts, delays and cross-thread channels work, see [Quench UI Native](./quench-ui-native.md#images-timers-and-threads)).

Limits: text only on the clipboard (no images, no drag and drop); only the primary button; one window; no decorations (a compositor that wants client-side decorations shows none).
