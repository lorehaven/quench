# Quench desktop Wayland

`quench-desktop-wayland` (crate `quench_desktop_wayland`) is a Wayland client written against the wire protocol, with no `libwayland` and no C: a window on the compositor, shared-memory frames to draw into, and pointer input. It is the window system under the [native renderer](../plans/NATIVE_RENDERER.md): [`quench-raster`](./quench-raster.md)'s `Canvas` draws straight into the frame the compositor reads, so a frame is never copied.

```rust
use quench_desktop_wayland::{Window, WindowEvent};
use quench_raster::Color;

let mut window = Window::open("Hello", "dev.example.hello", 640, 400)?;
loop {
    let mut canvas = window.begin_frame()?;        // waits for a free buffer
    canvas.clear(Color::rgb(30, 32, 38));
    canvas.fill_rounded_rect(24.0, 24.0, 200.0, 48.0, 8.0, Color::rgb(70, 130, 220));
    drop(canvas);
    window.present()?;                              // attach, damage, commit
    match window.next_event(None)? {
        Some(WindowEvent::CloseRequested) => break,
        Some(WindowEvent::Resized { .. } | WindowEvent::PointerButton { .. }) => {} // draw again
        _ => {}
    }
}
```

## The protocol subset

A message is `object id, size << 16 | opcode, arguments`, in the machine's byte order, each argument padded to 4 bytes; file descriptors travel beside the bytes (`SCM_RIGHTS`). `wire` is that format and nothing else (messages, string, array and 24.8 fixed-point arguments, a decoder that cuts a stream into messages however it arrives in pieces); `proto` has the opcodes and the decoding of events; `client` allocates object ids and reads events off the socket. A window uses:

| Interface | What for |
|---|---|
| `wl_display`, `wl_registry`, `wl_callback` | the handshake (`get_registry`, one `sync` round trip to learn the globals), frame callbacks |
| `wl_compositor`, `wl_surface` | the surface; `attach`, `damage_buffer`, `commit`, `frame` |
| `wl_shm`, `wl_shm_pool`, `wl_buffer` | two buffers of one memfd-backed pool, `ARGB8888` (premultiplied BGRA, the layout of `quench-raster`) |
| `xdg_wm_base`, `xdg_surface`, `xdg_toplevel` | the window: title, app id, configure and acknowledge, close, ping and pong |
| `wl_seat`, `wl_pointer` | enter, leave, motion, button (with the position of the last motion), axis |
| `wl_data_device_manager`, `wl_data_device`, `wl_data_source`, `wl_data_offer` | the **clipboard** (text only): `Window::set_clipboard(text)` offers `text/plain;charset=utf-8`, `text/plain` and `UTF8_STRING` as the selection, named by the serial of the last key or button, and answers other programs' `send` by writing to the descriptor they gave; `Window::clipboard()` returns this window's own text or reads the selection another program offers through a pipe (waiting up to a second for it to write) |

A compositor without `wl_compositor`, `wl_shm` or `xdg_wm_base` is refused by name; a missing seat is fine (no pointer).

## How the window behaves

- **Two buffers.** `begin_frame` hands out the one the compositor is not holding (it waits for a `wl_buffer.release` when both are held), and `present` attaches it, damages the whole surface and commits. The memory is the compositor's too, so a buffer is never written while it holds it.
- **Size.** The compositor chooses (`xdg_toplevel.configure`); 0 means "your choice" and the size asked for at `open` is used. A new size is applied when the configuration is acknowledged: new memory, the old buffers and pool destroyed, one `WindowEvent::Resized`. Sizes below 1 or above 16 384 are ignored.
- **Events.** `next_event(timeout)` returns `Resized`, `CloseRequested`, `PointerEntered`/`Left`/`Moved`, `PointerButton { x, y, button, pressed }` (Linux input codes: `0x110` left), `Scroll` and `Frame` (from `request_frame`). Pings are answered inside; nothing else needs handling.
- **Errors.** A protocol error the compositor reports, a closed connection, and a message that is not valid are `io::Error`s; after one the window is no use.

## The `unsafe`, and why it is in one file

The standard library cannot pass file descriptors over a socket (the API is nightly-only) or make shared memory, so `src/sys.rs` calls `sendmsg`/`recvmsg` with `SCM_RIGHTS`, `memfd_create`, `ftruncate`, `mmap` and `munmap` through `libc`. **Every `unsafe` of the crate is in that file**, on the security gate's allowlist with this reason, behind three safe types: `Socket`, `Mapping` and `memfd`. The contract (also at the top of the file):

- **Descriptors have one owner.** One that arrives becomes an `OwnedFd` at once (closed if nobody takes it); one that is sent is borrowed for the call.
- **A mapping lives as long as its value.** `Mapping` unmaps on drop and its slices borrow it. The memory is *shared*, which Rust cannot see: a window writes only the buffer the compositor has released, and treats what it reads back from someone else's memory as data.
- **Control data is sized and aligned** (a `u64` buffer for `cmsghdr`), and descriptors that did not all fit are an error, not a silent loss.

## How it is tested

The wire format, the system layer and the client are tested directly (every argument type and its padding, a decoder fed one byte at a time, a header that cannot be a message; descriptors and shared memory through a socket pair, a 4 MiB send completed while the other side reads, nine descriptors where eight fit; the client against a server that sends events, errors and nothing). **The window is tested against a compositor played by the test**: it answers the handshake, reads the pixels out of the pool's memory (so a drawn frame is checked as the compositor would see it), and sends events: the two-buffer discipline and its wait for a release, a resize (acknowledged, reallocated, old buffers destroyed, absurd sizes ignored), pings, pointer events, frame callbacks, close, a compositor that goes away, and one that lacks `wl_shm`. A last test opens a window on the real compositor (it skips, saying so, without one). 34 tests, 93% line coverage.

## Measured (slice 0 of the native renderer)

`node scripts/native-measure.mjs`, release builds, a window the compositor tiled to about 1916 x 2110, a few seconds after the first frame; the native window is a drawn frame (rounded rectangles, a stroke, an anti-aliased circle), the WebView window is the desktop host showing a one-line page.

| | RSS | PSS | processes |
|---|---|---|---|
| native (`quench-desktop-wayland` + `quench-raster`) | 18.5 MiB | 8.3 MiB | 1 |
| WebView host (`quench-desktop`) | 410.8 MiB | 206.1 MiB | 3 |

Drawing a full 1916 x 2110 frame (clear, rounded rectangles, a stroke, a circle) into the shared buffer takes a mean of 4.0 ms and a maximum of 9.3 ms over 120 frames, including the wait for the compositor to release a buffer. **Read these with care:** the native window shows no text yet (fonts are the next slice), no layout and no CSS; most of its memory is the two frame buffers (16 MiB each, shared with the compositor, which is why PSS is lower than RSS); and the comparison is a minimal window on each side, not an application.

## Not done

- **Keyboard and text input.** `wl_keyboard` needs xkb keymaps; text entry will use `text-input-v3` (the compositor sends the text), and shortcuts a minimal keymap.
- **HiDPI and fractional scale** (`wp-fractional-scale`, `wp-viewporter`): the window draws at scale 1, so a scaled output upscales it.
- **Decorations.** The window draws no title bar; a compositor that expects the client to draw one (GNOME) shows none. `xdg-decoration` or a drawn title bar is a later piece.
- **Cursor shape, clipboard, drag and drop, touch, the other seat devices, popups, fullscreen and maximise requests, X11.**
- Tested on one machine (Arch, Hyprland).


## Keyboard and input methods

`wl_keyboard`: the layout arrives as an XKB keymap in a file descriptor and [`Keymap::parse`](../quench-desktop-wayland/src/keymap.rs) reads what it needs (key names and codes, the symbols of each key's first group): four levels (plain, Shift, `AltGr`, `AltGr`+Shift; so `AltGr`+a is `ą` on a Polish layout), Caps Lock swapping the case of letters, and named keys as DOM `key` values (`Enter`, `ArrowLeft`, `Backspace`...). `WindowEvent::Key { code, pressed, key, modifiers }` carries it; the modifier masks use the standard bit positions. Not handled: dead keys, compose sequences and groups after the first; a keymap that cannot be read is the US layout. Key repeat is the program's job (`Window::key_repeat()` is what the compositor configured).

`zwp_text_input_v3`: `ime_enable(surrounding, cursor, anchor, rect)`, `ime_update(...)` and `ime_disable()` tell an input method which text field has focus, the text around its caret (at most 2000 bytes either side) and where the caret is on screen; `WindowEvent::Ime(ImeEvent)` delivers a change as one event when the compositor says `done`: delete around the caret, commit text, a composition. Tested against a played compositor; not against a real input method (no fcitx5 or ibus was run).
