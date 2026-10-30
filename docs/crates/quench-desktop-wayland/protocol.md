---
title: Protocol coverage
parent: quench-desktop-wayland
grand_parent: Crates
nav_order: 3
---

What the client binds, sends and understands. Everything not listed is not implemented (events of unknown objects or opcodes decode to `Event::Ignored`).

## Globals bound by `Window::open_on`

| Interface | Requested version | Required | Used for |
|---|---|---|---|
| `wl_compositor` | 4 | yes | `create_surface`. |
| `wl_shm` | 1 | yes | `create_pool`; format `ARGB8888` only (the `format` events are decoded, not used). |
| `xdg_wm_base` | 1 | yes | `get_xdg_surface`, `pong`. |
| `wl_seat` | 5 | no | `get_pointer`, `get_keyboard`. |
| `wl_data_device_manager` | 3 | no | clipboard. |
| `zwp_text_input_manager_v3` | 1 | no | input method. |

The version bound is `min(advertised, requested)`. A missing required global is `Unsupported` ("the compositor has no `<interface>`").

## Requests sent (opcode constants in `proto`)

| Object | Requests |
|---|---|
| `wl_display` | `sync` (0), `get_registry` (1) |
| `wl_registry` | `bind` (0) |
| `wl_compositor` | `create_surface` (0) |
| `wl_shm` | `create_pool` (0) with the memfd |
| `wl_shm_pool` | `create_buffer` (0), `destroy` (1) |
| `wl_buffer` | `destroy` (0) |
| `wl_surface` | `attach` (1), `frame` (3), `commit` (6), `damage_buffer` (9); constants `DESTROY` (0) and `DAMAGE` (2) are defined but unused |
| `xdg_wm_base` | `pong` (3), `get_xdg_surface` (2) |
| `xdg_surface` | `get_toplevel` (1), `ack_configure` (4) |
| `xdg_toplevel` | `set_title` (2), `set_app_id` (3); `set_min_size` (8) and `destroy` (0) defined but unused |
| `wl_seat` | `get_pointer` (0), `get_keyboard` (1) |
| `zwp_text_input_manager_v3` | `get_text_input` (1) |
| `zwp_text_input_v3` | `enable` (1), `disable` (2), `set_surrounding_text` (3), `set_content_type` (5), `set_cursor_rectangle` (6), `commit` (7) |
| `wl_data_device_manager` | `create_data_source` (0), `get_data_device` (1) |
| `wl_data_source` | `offer` (0), `destroy` (1) |
| `wl_data_device` | `set_selection` (1) |
| `wl_data_offer` | `receive` (1), `destroy` (2) |

Not sent: `wl_surface.set_buffer_scale`, `set_input_region`, `xdg_toplevel.set_maximized/fullscreen/move/resize/show_window_menu`, `xdg_toplevel.set_min_size`/`set_max_size`, cursor setting (`wl_pointer.set_cursor`), `wl_data_offer.accept`/`set_actions` (no drag and drop), `wl_keyboard.release`, `wl_pointer.release`, `text_input.set_text_change_cause`.

## Events decoded

| Object | Events |
|---|---|
| `wl_display` | `error`, `delete_id` |
| `wl_registry` | `global`, `global_remove` |
| `wl_callback` | `done` |
| `wl_shm` | `format` |
| `wl_buffer` | `release` |
| `xdg_wm_base` | `ping` |
| `xdg_surface` | `configure` |
| `xdg_toplevel` | `configure` (width, height, states array), `close` |
| `wl_seat` | `capabilities` (pointer bit `1`, keyboard bit `2`) |
| `wl_pointer` | `enter`, `leave`, `motion`, `button` (`state == 1` is pressed), `axis`, `frame` |
| `wl_keyboard` | `keymap` (format ignored, size read), `enter`, `leave`, `key`, `modifiers`, `repeat_info` |
| `zwp_text_input_v3` | `enter`, `leave`, `preedit_string`, `commit_string`, `delete_surrounding_text`, `done` |
| `wl_data_device` | `data_offer`, `selection` |
| `wl_data_offer` | `offer` |
| `wl_data_source` | `send`, `cancelled` |

How the window uses them: `XdgPing` is answered; `ToplevelConfigure` stores a suggested size (accepted only when both sides are 1..=16384); `XdgConfigure` acknowledges and applies the pending size (reallocating buffers and emitting `Resized`); `ToplevelClose` is `CloseRequested`; `BufferRelease` frees a slot; `SeatCapabilities` creates the pointer, keyboard, text-input and data-device objects once; `PointerEnter`, `PointerLeave`, `PointerMotion`, `PointerButton` (with the last position) and `PointerAxis` become window events (one per protocol event; `PointerFrame` is not used to group them); `KeyboardEnter` and keys and buttons store the serial for the clipboard; `PointerFrame`, `KeyboardLeave`, `TextInputEnter`, `TextInputLeave`, `GlobalRemove` and `ShmFormat` produce no window event. Only `axis` (axis, value) is decoded of the pointer axis events; `axis_source`, `axis_stop`, `axis_discrete` are ignored.

## Opcode constants

| Module | Constants (`u16` unless noted) |
|---|---|
| `display` | `SYNC` 0, `GET_REGISTRY` 1, `ERROR` 0, `DELETE_ID` 1 |
| `registry` | `BIND` 0, `GLOBAL` 0, `GLOBAL_REMOVE` 1 |
| `callback` | `DONE` 0 |
| `compositor` | `CREATE_SURFACE` 0 |
| `shm` | `CREATE_POOL` 0, `FORMAT` 0 |
| `shm_pool` | `CREATE_BUFFER` 0, `DESTROY` 1 |
| `buffer` | `DESTROY` 0, `RELEASE` 0 |
| `surface` | `DESTROY` 0, `ATTACH` 1, `DAMAGE` 2, `FRAME` 3, `COMMIT` 6, `DAMAGE_BUFFER` 9 |
| `xdg_wm_base` | `GET_XDG_SURFACE` 2, `PONG` 3, `PING` 0 |
| `xdg_surface` | `DESTROY` 0, `GET_TOPLEVEL` 1, `ACK_CONFIGURE` 4, `CONFIGURE` 0 |
| `xdg_toplevel` | `DESTROY` 0, `SET_TITLE` 2, `SET_APP_ID` 3, `SET_MIN_SIZE` 8, `CONFIGURE` 0, `CLOSE` 1 |
| `seat` | `GET_POINTER` 0, `GET_KEYBOARD` 1, `CAPABILITIES` 0, `POINTER: u32` 1, `KEYBOARD: u32` 2 |
| `text_input_manager` | `GET_TEXT_INPUT` 1 |
| `text_input` | `ENABLE` 1, `DISABLE` 2, `SET_SURROUNDING_TEXT` 3, `SET_CONTENT_TYPE` 5, `SET_CURSOR_RECTANGLE` 6, `COMMIT` 7, `ENTER` 0, `LEAVE` 1, `PREEDIT_STRING` 2, `COMMIT_STRING` 3, `DELETE_SURROUNDING_TEXT` 4, `DONE` 5 |
| `data_device_manager` | `CREATE_DATA_SOURCE` 0, `GET_DATA_DEVICE` 1 |
| `data_source` | `OFFER` 0, `DESTROY` 1, `SEND` 1, `CANCELLED` 2 |
| `data_device` | `SET_SELECTION` 1, `DATA_OFFER` 0, `SELECTION` 5 |
| `data_offer` | `RECEIVE` 1, `DESTROY` 2, `OFFER` 0 |
| `keyboard` | `KEYMAP` 0, `ENTER` 1, `LEAVE` 2, `KEY` 3, `MODIFIERS` 4, `REPEAT_INFO` 5 |
| `pointer` | `ENTER` 0, `LEAVE` 1, `MOTION` 2, `BUTTON` 3, `AXIS` 4, `FRAME` 5 |

Opcodes with the same number on different objects (for example `display::ERROR` and `display::SYNC`, both 0) are requests versus events; the module groups requests and events of one interface.

## Not supported

Server-side decorations (`xdg-decoration`), fractional scaling (`wp_fractional_scale`), `wp_viewporter`, `linux-dmabuf` / GPU buffers (shared memory only), `wl_output` (no monitor information), primary selection, drag and drop, touch (`wl_touch`), tablets, pointer constraints, `xdg_activation`, popups (`xdg_popup`), idle inhibit. The window is a plain toplevel with a client-drawn surface.
