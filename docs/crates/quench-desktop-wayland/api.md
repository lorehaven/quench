---
title: API reference
parent: quench-desktop-wayland
grand_parent: Crates
nav_order: 2
---

Modules in source order: crate root, `client`, `keymap`, `proto`, `sys`, `wire`, and the private `window` whose `ImeEvent`, `Window`, `WindowEvent` are re-exported at the root. All items exist only on Linux.

## Crate root

```rust
pub mod client;
pub mod keymap;
pub mod proto;
pub mod sys;
pub mod wire;
pub use keymap::{Key, Keymap, Modifiers};
pub use window::{ImeEvent, Window, WindowEvent};
```

### `trace_enabled`

```rust
#[must_use]
pub fn trace_enabled() -> bool
```

Whether `QUENCH_WAYLAND_TRACE` is set (cached on first call). Off by default.

### `trace_motion`

```rust
#[must_use]
pub fn trace_motion() -> bool
```

Counts calls (an `AtomicU64`); true for the first three and every 300th, so a trace shows a few motion events and then a sample. Each call counts, so it has a side effect despite the name.

## `window`

### `enum WindowEvent`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    Resized { width: usize, height: usize },
    CloseRequested,
    PointerEntered { x: f64, y: f64 },
    PointerLeft,
    PointerMoved { x: f64, y: f64 },
    PointerButton { x: f64, y: f64, button: u32, pressed: bool },
    Scroll { axis: u32, value: f64 },
    Key { code: u32, pressed: bool, key: Key, modifiers: Modifiers },
    Ime(ImeEvent),
    Frame,
}
```

| Variant | Meaning |
|---|---|
| `Resized` | New size in pixels; draw a frame of that size. The size at `open` is the first and is not announced. |
| `CloseRequested` | The user or desktop asked to close (`xdg_toplevel.close`). |
| `PointerEntered`, `PointerLeft`, `PointerMoved` | Pointer over the surface, surface-local coordinates. |
| `PointerButton` | `button` is the Linux input code (`0x110` left, `0x111` right, `0x112` middle); `x`, `y` are the last pointer position. |
| `Scroll` | `axis` 0 vertical, 1 horizontal; `value` in surface-local units, positive down/right. |
| `Key` | `code` is the Linux input code; `key` and `modifiers` are the layout's reading. Modifier keys are reported too. A key the layout does not have is not reported. |
| `Ime` | Input method change; apply in the order of `ImeEvent`. |
| `Frame` | The frame requested with `request_frame` is the compositor's next one. |

### `struct ImeEvent`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImeEvent {
    pub delete_before: u32,
    pub delete_after: u32,
    pub commit: Option<String>,
    pub preedit: Option<(String, i32, i32)>,
}
```

One change from the input method, delivered at the compositor's `done`. `delete_before`/`delete_after`: bytes to delete around the cursor first. `commit`: text to insert for good. `preedit`: the composition now (text, cursor begin, cursor end; byte offsets, negative for none); `Some("")` clears it.

### `struct Window`

```rust
pub struct Window { /* private */ }
```

A window on the compositor. `Debug` prints the size. `Window` is not `Send` or `Sync`: it owns a `Mapping`, which holds a raw pointer.

#### `Window::open`

```rust
pub fn open(title: &str, app_id: &str, width: usize, height: usize) -> io::Result<Self>
```

Opens a window on the compositor named by the environment (`Client::connect`). Errors: no compositor, the compositor lacks `wl_compositor`, `wl_shm` or `xdg_wm_base` (`ErrorKind::Unsupported`, message `the compositor has no <interface>`), no configuration within 5 s (`TimedOut`, `the compositor never configured the window`), or the connection's errors.

#### `Window::open_on`

```rust
pub fn open_on(mut client: Client, title: &str, app_id: &str, width: usize, height: usize) -> io::Result<Self>
```

As `open`, over a connection already made (used by tests with a fake compositor). Sequence: `get_registry`, roundtrip, bind (see [protocol](protocol.md)), create surface, `xdg_surface`, `xdg_toplevel`, set title and app id, commit without a buffer, wait for the first complete configuration (answering `ping`; other events are kept and replayed so a seat announced before the configure is not lost), acknowledge it, create buffers at the configured size (or the requested size when the compositor suggests 0 or an unacceptable size). The `seat`, `wl_data_device_manager` and `zwp_text_input_manager_v3` globals are optional.

#### `Window::size`

```rust
#[must_use]
pub const fn size(&self) -> (usize, usize)
```

The frame size in pixels.

#### `Window::next_event`

```rust
pub fn next_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<WindowEvent>>
```

The next window event, waiting up to `timeout` (`None`: forever). `Ok(None)` when the time ran out. Errors: the connection's (the compositor went away: `UnexpectedEof` "the compositor closed the connection"; a protocol error: `protocol error <code> on object <id>: <message>`; invalid data).

#### `Window::begin_frame`

```rust
pub fn begin_frame(&mut self) -> io::Result<Canvas<&mut [u8]>>
```

The canvas over the back buffer. If the compositor still holds it, events are handled until it is released, for at most 5 s per wait (`TimedOut`, `the compositor did not release a buffer`). The canvas is `width x height`, 4 bytes per pixel in `ARGB8888` (premultiplied, little-endian `0xAARRGGBB`, i.e. BGRA in memory). The previous contents are those of two frames ago. Panics (`expect`) only if `width * height * 4` does not match the slice, which does not happen.

*Documentation note:* in the source the doc comment for this method sits above `key_repeat` (a misplaced block); see the gap report.

#### `Window::present`

```rust
pub fn present(&mut self) -> io::Result<()>
```

Shows the frame drawn after `begin_frame`: `attach`, `damage_buffer` (whole buffer), `commit`; marks the slot busy and swaps to the other slot.

#### `Window::request_frame`

```rust
pub fn request_frame(&mut self) -> io::Result<()>
```

Asks for a `wl_surface.frame` callback; the callback's `done` becomes `WindowEvent::Frame`. Call it before `present`.

#### `Window::key_repeat`

```rust
#[must_use]
pub const fn key_repeat(&self) -> (i32, i32)
```

Characters per second and the delay before repeating in milliseconds, as the compositor configured (`wl_keyboard.repeat_info`). Default `(25, 600)` until the compositor says. The compositor does not repeat keys itself.

#### `Window::set_clipboard`

```rust
pub fn set_clipboard(&mut self, text: &str) -> io::Result<()>
```

Becomes the clipboard selection owner: destroys the previous source, creates a `wl_data_source`, offers the MIME types `text/plain;charset=utf-8`, `text/plain`, `UTF8_STRING`, and sets the selection with the serial of the last input event. Does nothing (returns `Ok(())`) without a data device manager or a data device (no seat).

#### `Window::clipboard`

```rust
pub fn clipboard(&mut self) -> io::Result<Option<String>>
```

The text this program put on the clipboard, else the current selection offer's text: picks the first offered type among `text/plain;charset=utf-8`, `text/plain`, `UTF8_STRING`, `TEXT`, `STRING`, asks for it over a pipe and waits up to 1 s; reads at most `1 << 24` bytes; invalid UTF-8 is replaced lossily. `Ok(None)` if there is no selection, no text type, or the other program is too slow.

#### `Window::has_ime`

```rust
#[must_use]
pub const fn has_ime(&self) -> bool
```

Whether a `zwp_text_input_v3` object exists (the compositor offers `zwp_text_input_manager_v3` and a seat announced capabilities).

#### `Window::ime_enable`

```rust
pub fn ime_enable(&mut self, surrounding: &str, cursor: i32, anchor: i32, rect: (i32, i32, i32, i32)) -> io::Result<()>
```

Tells the input method a text field has focus: sends `enable`, then `set_surrounding_text` (at most 2000 bytes either side of `cursor`, cut on character boundaries; offsets shifted accordingly and clamped at 0), `set_content_type(0, 0)` (no hint, normal purpose), `set_cursor_rectangle(x, y, w, h)` and `commit`. No-op without an input method.

#### `Window::ime_update`

```rust
pub fn ime_update(&mut self, surrounding: &str, cursor: i32, anchor: i32, rect: (i32, i32, i32, i32)) -> io::Result<()>
```

As `ime_enable` without the `enable` request.

#### `Window::ime_disable`

```rust
pub fn ime_disable(&mut self) -> io::Result<()>
```

Sends `disable` and `commit`. No-op without an input method.

## `client`

### `struct Client`

```rust
#[derive(Debug)]
pub struct Client { /* socket, decoder, fds, interfaces, next_id, ready */ }
```

A client of the compositor: allocates object ids (from 2; the display is 1), sends requests and decodes events by the interface registered for the id.

* `pub fn connect() -> io::Result<Self>`: connects with `Socket::connect_display`.
* `#[must_use] pub fn new(socket: Socket) -> Self`: over an existing socket.
* `pub fn new_id(&mut self, interface: Interface) -> u32`: a fresh id; its events are decoded as `interface`.
* `pub fn send(&self, object: u32, opcode: u16, args: Args, fds: &[BorrowedFd<'_>]) -> io::Result<()>`: errors: the socket's, or a request longer than 65535 bytes (`InvalidData`, `a message cannot be longer than 65535 bytes`).
* `pub fn take_fd(&mut self) -> Option<OwnedFd>`: the oldest descriptor that arrived and has not been claimed (what a keymap message refers to).
* `pub fn next_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<(u32, Event)>>`: the next event with the id of its object; `Ok(None)` on timeout (a timeout below 1 ms is raised to 1 ms). Errors: the socket's, `UnexpectedEof` for a closed connection, `InvalidData` for undecodable messages, and `io::Error::other("protocol error {code} on object {object}: {message}")` for a `wl_display.error` event. `DeleteId` forgets the id; a `DataOffer(id)` registers `id` as a data-offer interface.
* `pub fn roundtrip(&mut self) -> io::Result<Vec<(u32, Event)>>`: sends `wl_display.sync` and returns what arrived before its callback fired. Gives up after 5 s without any event (`TimedOut`, `the compositor did not answer`).

## `keymap`

Reads an XKB keymap (text form) into per-key symbol lists. Four levels (plain, Shift, AltGr, AltGr+Shift); Caps Lock swaps the case of letters only. Dead keys, compose sequences and groups after the first are not handled.

### `struct Modifiers`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers { pub shift: bool, pub caps: bool, pub ctrl: bool, pub alt: bool, pub altgr: bool, pub meta: bool }
```

`pub const fn from_masks(depressed: u32, latched: u32, locked: u32) -> Self` (`#[must_use]`): ORs the three masks and reads the standard bits: Shift = bit 0 (`1`), Lock/caps = 1 (`2`), Control = 2 (`4`), Mod1/Alt = 3 (`8`), Mod4/Super = 6 (`64`) -> `meta`, Mod5/level three = 7 (`128`) -> `altgr`.

### `struct Key`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key { pub name: String, pub text: Option<String>, pub modifier: bool }
```

A key press in the terms of a DOM `KeyboardEvent`: `name` is the DOM `key` value (`"a"`, `"Ą"`, `"Enter"`, `"ArrowLeft"`, `"Shift"`), `text` the text it types (if any), `modifier` true for Shift, Control, Alt, Super/Meta, AltGr, Caps Lock, Num Lock.

### `struct Keymap`

```rust
#[derive(Debug, Clone, Default)]
pub struct Keymap { /* by Linux input code: keysym names per level */ }
```

* `#[must_use] pub fn us() -> Self`: the US layout (a fixed table of 84 key codes: Escape, digits, letters, punctuation, Backspace, Tab, Return, Control, Shift, Alt, Space, Caps Lock, F1-F12, KP_Enter, Control_R, ISO_Level3_Shift, Home, arrows, Page keys, Insert, Delete, Super). Used when no keymap arrives or it cannot be read.
* `#[must_use] pub fn parse(text: &str) -> Option<Self>`: reads `xkb_keycodes` (names, numeric codes and aliases) and `xkb_symbols` (first group's first symbol list per key; the XKB code minus 8 is the Linux code). Symbols written as hex numbers (`0x6c`, `0xff0d`) are turned into names or characters. `None` if no key could be read.
* `#[must_use] pub fn key(&self, code: u32, mods: Modifiers) -> Option<Key>`: the key `code` gives with `mods`. Level = Shift + 2 if AltGr; falls back to the Shift-less pair, then level 0, when the level is missing or `NoSymbol`; with Caps Lock at levels 0-1 the other case is chosen for letters. `None` for a code the layout lacks. Key names mapped: Return/KP_Enter/ISO_Enter -> `Enter`, BackSpace -> `Backspace`, Tab/ISO_Left_Tab -> `Tab`, Escape, Delete/KP_Delete -> `Delete`, Insert, Home, End, Left/Right/Up/Down -> `ArrowLeft/Right/Up/Down`, Prior/Page_Up -> `PageUp`, Next/Page_Down -> `PageDown`, Shift_L/R -> `Shift`, Control_L/R -> `Control`, Alt_L/R -> `Alt`, ISO_Level3_Shift/Mode_switch -> `AltGraph`, Super/Meta -> `Meta`, Caps_Lock -> `CapsLock`, Num_Lock -> `NumLock`, `F<n>` -> `F<n>`; anything else is the text of the keysym if it has one.

### `keysym_text`

```rust
#[must_use]
pub fn keysym_text(name: &str) -> Option<String>
```

The text a key symbol types: a table of named symbols (`space`, `exclam`, ... punctuation), a single character as itself, `KP_0`..`KP_9` as the digit, `U<hex>` as that code point. `None` otherwise.

## `proto`

The parts of the core and `xdg-shell` protocols this client speaks; see [protocol coverage](protocol.md) for all opcode constants.

* `pub const DISPLAY: u32 = 1;`: the object every connection starts with.
* `pub const FORMAT_ARGB8888: u32 = 0;`: `wl_shm` format.
* `pub mod display, registry, callback, compositor, shm, shm_pool, buffer, surface, xdg_wm_base, xdg_surface, xdg_toplevel, seat, text_input_manager, text_input, data_device_manager, data_source, data_device, data_offer, keyboard, pointer`: `pub const <NAME>: u16` opcodes (and `seat::POINTER`/`KEYBOARD` capability bits, `u32`).
* `pub enum Interface`: `Display, Registry, Callback, Compositor, Shm, ShmPool, Buffer, Surface, XdgWmBase, XdgSurface, XdgToplevel, Seat, Pointer, Keyboard, TextInputManager, TextInput, DataDeviceManager, DataDevice, DataSource, DataOffer` (`Debug, Clone, Copy, PartialEq, Eq`): what kind of object an id is.
* `pub enum Event` (`Debug, Clone, PartialEq`): `Error { object, code, message }`, `DeleteId(u32)`, `Global { name, interface, version }`, `GlobalRemove(u32)`, `CallbackDone`, `ShmFormat(u32)`, `BufferRelease`, `XdgPing { serial }`, `XdgConfigure { serial }`, `ToplevelConfigure { width: i32, height: i32, states: Vec<u32> }`, `ToplevelClose`, `SeatCapabilities(u32)`, `PointerEnter { serial, x, y }`, `PointerLeave`, `PointerMotion { x, y }`, `PointerButton { serial, button, pressed }`, `PointerAxis { axis, value }`, `PointerFrame`, `KeyboardKeymap { size }`, `KeyboardEnter { serial }`, `KeyboardLeave`, `TextInputEnter`, `TextInputLeave`, `TextInputPreedit { text, cursor_begin, cursor_end }`, `TextInputCommit(String)`, `TextInputDeleteSurrounding { before, after }`, `TextInputDone`, `KeyboardKey { serial, key, pressed }`, `DataOffer(u32)`, `OfferType(String)`, `Selection(Option<u32>)`, `SourceSend { mime }`, `SourceCancelled`, `KeyboardModifiers { depressed, latched, locked }`, `KeyboardRepeatInfo { rate: i32, delay: i32 }`, `Ignored`.
* `pub fn decode(interface: Interface, message: &Message) -> Result<Event, WireError>`: reads one event of an object of `interface` from its opcode and arguments; an event this client does not use is `Event::Ignored`. Errors: arguments that are not what the protocol says.

## `sys`

Everything the standard library cannot do for a Wayland client. **All `unsafe` of the crate is in this file** (`#![allow(unsafe_code)]`), each block with a `SAFETY` comment. Contract: descriptors have one owner (received ones become `OwnedFd` at once; sent ones are borrowed for the call); a `Mapping` lives as long as its value; the memory is shared with the compositor, so do not write a buffer it may be reading.

### `struct Socket`

```rust
#[derive(Debug)]
pub struct Socket { /* UnixStream */ }
```

* `pub fn connect_display() -> io::Result<Self>`: connects to `$WAYLAND_DISPLAY` (default `wayland-0`), relative to `$XDG_RUNTIME_DIR` unless absolute.
* `pub fn connect_path(path: &Path) -> io::Result<Self>`.
* `#[must_use] pub const fn from_stream(stream: UnixStream) -> Self`.
* `pub fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>`.
* `pub fn send(&self, data: &[u8], fds: &[BorrowedFd<'_>]) -> io::Result<()>`: `sendmsg` with `MSG_NOSIGNAL`, descriptors in `SCM_RIGHTS` with the first byte; a short write is completed with `write_all`; `EINTR` is retried.
* `pub fn recv(&self, buf: &mut [u8]) -> io::Result<(usize, Vec<OwnedFd>)>`: `recvmsg` with `MSG_CMSG_CLOEXEC`; zero bytes is the end of the connection; room for 8 descriptors per call; more is an error (`descriptors were lost: more than fit in one message`).

### `memfd`

```rust
pub fn memfd(len: usize) -> io::Result<OwnedFd>
```

An anonymous zero-filled file of `len` bytes (`memfd_create` named `quench-wayland`, `MFD_CLOEXEC`, `ftruncate`). Errors: the kernel's, or `the buffer is too large`.

### `struct Mapping`

```rust
#[derive(Debug)]
pub struct Mapping { /* ptr, len, writable */ }
```

A shared mapping, unmapped on drop.

* `pub fn new(fd: BorrowedFd<'_>, len: usize, writable: bool) -> io::Result<Self>`: `MAP_SHARED`; `len == 0` is `InvalidInput` ("cannot map 0 bytes").
* `#[must_use] pub const fn len(&self) -> usize`, `#[must_use] pub const fn is_empty(&self) -> bool`.
* `#[must_use] pub const fn as_slice(&self) -> &[u8]`.
* `pub fn as_mut_slice(&mut self) -> &mut [u8]`: panics (`this mapping is read-only`) when the mapping is not writable.

## `wire`

The Wayland wire format. Header: 32-bit object id, a 32-bit word (total size in the upper 16 bits, opcode in the lower 16), then arguments padded to 4 bytes. Integers are in the machine's byte order. Descriptors travel beside the bytes.

* `pub const HEADER: usize = 8;`, `pub const MAX_MESSAGE: usize = 0xFFFF;`
* `pub enum WireError` (`Debug, Clone, Copy, PartialEq, Eq`, implements `Display` and `Error`): `BadSize(usize)` ("a message of N bytes is not valid"), `Short` ("an argument runs past the end of its message"), `BadString` ("a string argument is not valid"), `TooLong` ("a message cannot be longer than 65535 bytes").
* `pub struct Message { pub object: u32, pub opcode: u16, pub args: Vec<u8> }` (`Debug, Clone, PartialEq, Eq`).
* `pub struct Args(Vec<u8>)` (`Debug, Default, Clone`): builder with `#[must_use] const fn new()`, `uint(u32)`, `int(i32)`, `fixed(f64)` (24.8 fixed point, rounded), `object(u32)`, `string(&str)` (length including NUL, bytes, NUL, padding), `array(&[u8])` (length, bytes, padding), `build() -> Vec<u8>`.
* `pub fn encode(object: u32, opcode: u16, args: &[u8]) -> Result<Vec<u8>, WireError>`: `TooLong` if header + args exceed 65535. Debug-asserts that args are a multiple of 4.
* `pub struct Decoder` (`Debug, Default`): `const fn new()`, `feed(&mut self, bytes: &[u8])`, `next_message(&mut self) -> Result<Option<Message>, WireError>`: the next whole message or `None` until the rest is fed; `BadSize` for a size below the header or not a multiple of 4 (the stream is then untrustworthy).
* `pub struct Reader<'a>` (`Debug`): `const fn new(data: &'a [u8])`; `uint`, `int`, `fixed` (÷ 256), `object`, `string` (a length of 0 reads as empty; `BadString` without NUL or non-UTF-8), `array` (`&'a [u8]`) each returning `Result<_, WireError>` (`Short` when out of arguments); `const fn done(&self) -> bool`.
