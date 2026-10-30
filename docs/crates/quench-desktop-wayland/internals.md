---
title: Internals
parent: quench-desktop-wayland
grand_parent: Crates
nav_order: 4
---

## Buffers

`Buffers::create` makes one memfd of `2 * width * height * 4` bytes, maps it shared and read-write, creates one `wl_shm_pool` from it (passing the descriptor with `create_pool`) and two `wl_buffer`s (offset `i * frame`, stride `width * 4`, format `ARGB8888`), then drops its own descriptor. A slot is `busy` from `present` until the compositor's `wl_buffer.release`. `begin_frame` waits for the back slot to be free. A new size allocates fresh buffers, destroys the old buffers and pool and resets the back slot to 0. The memfd is zero-filled by the kernel and not touched.

Largest side: `MAX_SIDE = 16384`; a suggested size above that (or non-positive) is ignored (0 means "your choice").

## Handshake order

1. `get_registry`, roundtrip (collect globals).
2. Bind `wl_compositor`, `wl_shm`, `xdg_wm_base`, `wl_seat`, `wl_data_device_manager`, `zwp_text_input_manager_v3`.
3. `create_surface`, `get_xdg_surface`, `get_toplevel`, `set_title`, `set_app_id`, empty `commit`.
4. Read events (5 s limit without any) until `xdg_surface.configure`; `ping` is answered; the toplevel size is taken if acceptable; everything else is kept and replayed after the window exists, so a seat announced before the configure still gets its pointer and keyboard.
5. `ack_configure`, buffers, replay.

## Event queue

`Window` keeps a `VecDeque<WindowEvent>`. `next_event` returns queued events first; otherwise it asks the `Client` for the next protocol event. The input-method events accumulate in one `ImeEvent` until `done`.

## Keymap

On `wl_keyboard.keymap` the descriptor is read from offset 0 with `read_at` (the offset is shared with the compositor's write position, so a plain read finds nothing), trailing NULs are trimmed, and `Keymap::parse` is tried. If it fails the previous keymap stays (initially `Keymap::us()`). Key events are mapped with the modifiers last reported by `wl_keyboard.modifiers`.

## Clipboard

Own selection: a `wl_data_source` offering three text MIME types; on `send` the text is written to the descriptor if the MIME type is a text type (`text/plain;charset=utf-8`, `text/plain`, `UTF8_STRING`, `TEXT`, `STRING`); on `cancelled` the source is destroyed. Foreign selection: offers are tracked with their types; the previous offer is destroyed when the selection changes.

## Limits

* Message at most 65535 bytes.
* Descriptors per `recvmsg`: 8.
* Receive buffer per read: 4096 bytes.
* Connect/handshake waits: 5 s; buffer release wait 5 s; clipboard read 1 s, at most 16 MiB.
* Surrounding text for the input method: 2000 bytes either side of the cursor.

## Safety

`sys.rs` is the only file with `unsafe`. The shared memory is written by the program and read by the compositor; the double buffer ensures the program writes only a buffer it was told is released. A `Mapping` borrowed slice cannot outlive the mapping.
