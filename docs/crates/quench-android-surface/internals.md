---
title: Protocol and behaviour
parent: quench-android-surface
grand_parent: Crates
nav_order: 3
---

## Protocol

Every message is `length (u32) | kind (u8) | payload`; `length` counts the kind byte and the payload. All numbers are big-endian (what Java's `DataInputStream` reads). A *short string* has a `u16` length and UTF-8 bytes; a *long string* has a `u32` length. The largest message accepted from the host is `1 << 26` bytes (64 MiB).

### Host to program

| Kind | Event | Payload |
|---|---|---|
| 1 | `Resize` | `u32 width, u32 height, f32 density, f32 top, f32 right, f32 bottom, f32 left` |
| 2 | `Touch` | `u8 action, f32 x, f32 y` |
| 3 | `Key` | `u8 down (0/1), u8 mods, short string name` |
| 4 | `Text` | short string |
| 5 | `Compose` | short string text, `i32 cursor` |
| 6 | `Delete` | `u32 before, u32 after` |
| 7 | `Access` | `u32 id, u8 action` |
| 8 | `Scheme` | `u8 dark` |
| 9 | `Back` | none |
| 10 | `Visible` | `u8` |
| 11 | `Clipboard` | long string |
| 12 | `Reply` | long string (JSON envelope) |

### Program to host

| Kind | `Out` | Payload |
|---|---|---|
| 1 | `Frame` | `u32 width, u32 height`, then `width*height*4` bytes RGBA |
| 2 | `Keyboard` | `u8 show` |
| 3 | `Ime` | long string text, `i32 cursor, i32 anchor`, `f32 x, y, w, h` |
| 4 | `Tree` | the bytes of `encode_tree` |
| 5 | `BackHandled` | none |
| 6 | `Exit` | none |
| 7 | `Clipboard` | long string |
| 8 | `Call` | long string (JSON envelope) |

### The accessibility tree

`encode_tree` output: `u32 count`, then per node, depth-first in document order:

| Field | Type |
|---|---|
| id | `u32` |
| parent id | `u32` (`0xFFFF_FFFF` for the root) |
| role code | `u8` |
| flags | `u16` |
| heading level | `u8` |
| bounds x, y, w, h | 4 x `f32` |
| name, value, description | 3 short strings, each truncated to at most 4000 bytes on a character boundary (value is `""` when `None`) |
| range flag | `u8`; if `1`: `f32 min, f32 max, f32 now` (`NaN` while indeterminate) |

Flag bits: 0 focusable, 1 focused, 2 disabled, 3 checkable, 4 checked, 5 selectable, 6 selected, 7 expandable, 8 expanded, 9 required, 10 read-only, 11 can click, 12 can focus, 13 can scroll into view.

Role codes: 0 Document, 1 Heading, 2 Paragraph, 3 Text, 4 Button, 5 Link, 6 Checkbox, 7 Radio, 8 Switch, 9 TextBox, 10 List, 11 ListItem, 12 Tab, 13 TabList, 14 TabPanel, 15 Dialog, 16 Navigation, 17 Status, 18 Alert, 19 Image, 20 Form, 21 Group, 22 Slider, 23 ProgressBar.

### Plugin envelopes

Out (kind 8): `{"id": n, "plugin": "...", "method": "...", "args": ...}`. In (kind 12): `{"id": n, "ok": value}`, `{"id": n, "err": {"code", "message"}}` or `{"event": "...", "plugin": "...", "data": ...}`. These are the same envelopes a web page uses; an unparseable envelope is dropped.

## Touch

Constants (`src/lib.rs`): slop 8 CSS px (scaled by density), long press 500 ms, press delay 80 ms, handle reach 28 CSS px, handle knob 9 CSS px, fling window 100 ms, minimum fling speed 150 device px/s, friction 3.5 per second, stop speed 25 px/s, fling step 16 ms, caret blink 530 ms.

1. **Down (0)**: stops a fling; if the finger is within reach of a selection handle's knob, the touch drags that end of the selection (nothing else happens with it).
2. **Move (1)**: past the slop, and unless long-pressed, the direction decides: if the page's `touch-action` at the start point allows the direction (`touch_action_at(..).allows(dx, dy)`), the touch becomes a scroll (`wheel` with the deltas; a press already sent is cancelled); otherwise the page owns the gesture (`pointer_down` if not yet sent, then `pointer_drag` on each move). Samples from the last 100 ms are kept for the fling.
3. **Up (2)**: an owned gesture ends with `pointer_release`; a scroll starts a fling if the speed over the last 100 ms is at least 150 px/s (and at least 5 ms of samples); a long press or handle drag ends without a click (press cancelled); otherwise it is a tap: `pointer_down` (if not already sent after 80 ms) and `pointer_up`. Afterwards the pointer is moved to (-1, -1) so no `:hover` is left.
4. **Cancel (3 or other)**: `pointer_cancel` if a touch was active; pointer to (-1, -1).

The press is delayed 80 ms so a scroll that starts sooner never presses (a press restyles the page and would delay the first movement by a layout).

## Fling

On each 16 ms step the content moves by `-velocity * dt` through `wheel` (`dt` capped at 50 ms), then the velocity is multiplied by `e^(-3.5 * dt)`; it stops below 25 px/s.

## Selection

A finger held 500 ms selects the word under it. Handles are drawn into the frame (a stem along the caret and a knob below it). Dragging a knob calls `move_selection_end(end, x)` (`false` = start, `true` = end) and may cross the other end. No floating menu; copy, cut and paste use Ctrl+C, X, V and the clipboard messages.

## Frames

A frame is produced only when the renderer reports it is dirty, the size is known and the app is visible. The canvas is reused while the size is unchanged. The BGRA canvas is swapped to RGBA in place before sending.

## Input method

`Out::Keyboard(true)` precedes the first `Ime` when a field gets focus; `Keyboard(false)` is sent when the state becomes `None`. `Ime` is re-sent whenever `ime_state()` differs from the last one sent (compared after each frame). Offsets are UTF-16 units; `move_utf16` and `utf16_len` (private) convert.

## Environment

`QUENCH_FRAME_STATS`: if set (any value), per-second frame timing to standard error.
