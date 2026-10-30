---
title: Behaviour
parent: quench-desktop-native
grand_parent: Crates
nav_order: 3
---

## Timers

Caret blink: 530 ms (`BLINK`). Wheel: `WHEEL_SCALE = 4.0` pixels per unit (the compositor sends about 10 per notch). Left button: Linux code `0x110`.

## Frames

A frame is drawn only when `take_dirty` is true. The window's buffer is `ARGB8888`; `NativeRenderer::render` draws into the `Canvas` over the shared memory.

## Input method

After each `step`, `sync_ime` compares the renderer's field state with what the input method was last told: first focus -> `ime_enable`, later changes -> `ime_update`, focus lost -> `ime_disable`. The caret rectangle is the renderer's `rect` truncated to integers; cursor and anchor are byte offsets.

## Accessibility

When the service started, the tree is published after the first frame and after each frame that drew. `take_asked` makes the loop poll every 25 ms for three seconds so answers are quick; `take_actions` are carried out with `renderer.perform(id, action)` and `take_edits` with `renderer.set_field_value(id, text)`.

## Key repeat

The compositor leaves repeating to the program. The first repeat is `delay` ms after the press; later ones every `1/rate` s (rate below 1 counts as 1). Only keys that type text and Backspace, Delete and the arrow keys repeat; a press of another key replaces the held key; releasing the held key stops it. Repeated presses go through `key_down` (and `text` for printable keys) only; no `key_up` between repeats.

## Not handled

Middle and right mouse buttons, drag and drop, window scale factors, cursor shapes, window state requests (maximize, fullscreen), the `Frame` event (the loop does not use frame callbacks).
