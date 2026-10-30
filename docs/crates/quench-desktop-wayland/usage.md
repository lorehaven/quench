---
title: Usage
parent: quench-desktop-wayland
grand_parent: Crates
nav_order: 1
---

*These examples open real windows; run them only in a desktop session. This book's checks did not run any.*

## 1. Open, draw and close

See the minimal example on the [overview](index.md). `Window::open(title, app_id, width, height)` connects, binds the globals, creates an `xdg_toplevel`, waits for the first configuration (the compositor may choose another size; `size()` tells it) and returns. `begin_frame` hands out a `Canvas<&mut [u8]>` over the buffer that is free; the previous contents are what was drawn two frames ago, so clear what you do not cover. Drop the canvas, then `present`.

## 2. Draw on the compositor's schedule

```rust
window.request_frame()?;       // before present
window.present()?;
loop {
    match window.next_event(None)? {
        Some(WindowEvent::Frame) => { /* draw, request_frame, present */ break; }
        Some(WindowEvent::Resized { width, height }) => { /* next frame has the new size */ }
        _ => {}
    }
}
```

`WindowEvent::Frame` arrives when the compositor's `wl_surface.frame` callback fires. Without it, a loop that presents continuously is limited only by buffer release (`begin_frame` waits up to 5 s for a free buffer).

## 3. Idle with a timeout

`next_event(Some(d))` returns `Ok(None)` after `d`. This is how a host runs timers (caret blink, key repeat): wait for the earlier of the next timer and the next event.

## 4. Keyboard

```rust
if let Some(WindowEvent::Key { code, pressed, key, modifiers }) = window.next_event(None)? {
    if pressed && !key.modifier {
        if let Some(text) = &key.text { /* typed text */ }
        // key.name is the DOM `key`: "a", "Ą", "Enter", "ArrowLeft"
    }
}
let (rate, delay_ms) = window.key_repeat();   // the compositor does not repeat keys: the program does
```

Pitfall: modifier keys are reported too (`key.modifier == true`); ignore them for text.

## 5. Scroll and pointer

`PointerMoved`/`PointerEntered` positions are surface-local `f64`. `PointerButton.button` is the Linux input code (`0x110` left, `0x111` right, `0x112` middle). `Scroll { axis, value }`: `axis` 0 vertical, 1 horizontal, positive down/right.

## 6. Clipboard

```rust
window.set_clipboard("copied text")?;
if let Some(text) = window.clipboard()? { /* text from another program or this one */ }
```

`clipboard()` waits up to one second for the other program to write; reads at most 16 MiB (`1 << 24`). Without `wl_data_device_manager` or a seat both calls do nothing (`set_clipboard` returns `Ok(())`, `clipboard` returns `Ok(None)`).

## 7. Input method (IME)

```rust
if window.has_ime() {
    window.ime_enable("text around caret", cursor_byte, anchor_byte, (x, y, w, h))?;
    // on change:
    window.ime_update(text, cursor, anchor, rect)?;
    // on blur:
    window.ime_disable()?;
}
// events:
if let Some(WindowEvent::Ime(change)) = window.next_event(None)? {
    // apply in order: delete_before/delete_after, then commit, then preedit
}
```

Offsets are bytes. Surrounding text is cut to at most 2000 bytes each side of the cursor.

## 8. Debugging input

`QUENCH_WAYLAND_TRACE=1 your-program` prints the seat capabilities, keymap result (`keymap of N bytes: read ok, parsed` or `NOT parsed (US layout used)`; an unreadable keymap is also written to `quench-keymap.xkb` in the temp directory), and pointer and key events (pointer motion is thinned: the first three events and every 300th).
