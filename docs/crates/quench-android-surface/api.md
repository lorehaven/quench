---
title: API reference
parent: quench-android-surface
grand_parent: Crates
nav_order: 2
---

Source: `src/lib.rs`, `src/proto.rs`, `src/bridge.rs`. The crate has two public modules, `bridge` and `proto`.

## Crate root

### Re-exports

```rust
pub use bridge::{Bridge, CallError};
pub use quench_serde::{Map, Value};
```

`Map` and `Value` are the JSON object and value types of a call's arguments and answer.

### `struct Surface<W: Write>`

```rust
pub struct Surface<W: Write> { /* private */ }
```

The state of the program: the renderer, the output stream and what was last told to the host (size, density, touch, fling, colour scheme, visibility, input-method state, last tree, caret blink, the bridge, the last canvas). Implements `Debug` (prints `size` only). Not `Send` (holds an `Rc`). Private fields are not documented.

#### `Surface::new`

```rust
pub fn new(renderer: &Rc<NativeRenderer>, out: W) -> Self
```

A program that draws `renderer` and writes to `out`, with an empty `Bridge`. Reads `QUENCH_FRAME_STATS` (see [usage](usage.md#4-frame-timing)).

#### `Surface::with_bridge`

```rust
pub fn with_bridge(renderer: &Rc<NativeRenderer>, out: W, bridge: Bridge) -> Self
```

As `new`, with calls to the host's plugins going through `bridge`.

#### `Surface::finished`

```rust
#[must_use]
pub const fn finished(&self) -> bool
```

True once the page asked to exit. *Note:* in the code the flag `exit` is initialised `false` and no code path in `lib.rs` sets it; `Back` writes `Out::Exit` to the host instead. In practice the loop ends when the input closes. See the gap report.

#### `Surface::handle`

```rust
pub fn handle(&mut self, event: Event)
```

Does what an event from the host asks. Per event:

| Event | Effect |
|---|---|
| `Resize` | Stores size; density clamped to at least `0.5`; sets the renderer's scale, viewport (width, height as `f32`) and safe area (insets); re-applies media. Prints `surface: WxH at density D, insets [..]` to standard error. |
| `Touch` | The touch state machine (see [internals](internals.md#touch)). |
| `Key { down, mods, name }` | Builds a `KeyEvent` (`mods` bits: 1 shift, 2 ctrl, 4 alt, 8 meta). Key down calls `key_down`; a name of exactly one character with no ctrl, alt or meta is also sent as text (`renderer.text`). Key up calls `key_up`. |
| `Text(t)` | `ime_apply(0, 0, Some(t), None)`: commit. |
| `Compose { text, cursor }` | Composition; a negative `cursor` is passed as byte offset `-1`, otherwise the UTF-16 `cursor` is converted to a byte offset in `text`. |
| `Delete { before, after }` | Reads the focused field (`ime_state`), converts the UTF-16 counts to bytes around the caret and applies a deletion. |
| `Access { id, action }` | `renderer.perform(id, action)` when `action_of(action)` is `Some`; unknown codes are ignored. |
| `Scheme { dark }` | Stores the scheme and re-applies media. |
| `Back` | Sends `Escape` key-down to the page. If the page used it, writes `Out::BackHandled`, otherwise `Out::Exit`. Write errors are ignored. |
| `Visible(v)` | Frames are not sent while `false`. |
| `Clipboard(t)` | `renderer.set_clipboard(t)`. |
| `Reply(json)` | Delivered to the bridge (answer or event). |

Media applied: `hover: false`, `pointer: Pointer::Coarse`, colour scheme dark or light (initially dark), other fields from `MediaEnv::default()`.

#### `Surface::next_timer`

```rust
#[must_use]
pub fn next_timer(&self) -> Instant
```

The next instant `timers` has work: the caret blink (`530 ms`), the next fling step (`16 ms` after the last), and for a finger that is down and neither scrolling, owned, long nor on a handle: the long-press time (`500 ms`) and, if the page has not yet been told the finger is down, the press delay (`80 ms`).

#### `Surface::timers`

```rust
pub fn timers(&mut self, now: Instant)
```

Runs the timers that are due: press after 80 ms; long press after 500 ms (cancels the press and selects the word under the finger with `select_word_at`); a fling step; the caret blink (toggles; `set_caret_visible` only while a text field has focus).

#### `Surface::flush`

```rust
pub fn flush(&mut self) -> io::Result<()>
```

Writes what changed, in this order: queued plugin calls (`Out::Call`), clipboard text the page copied (`Out::Clipboard`); then, if a size is known, the app is visible and the renderer is dirty: a frame (`Out::Frame`), the soft-keyboard request (`Out::Keyboard(true)` when a field gets focus, `Out::Keyboard(false)` when focus is lost) with the field state (`Out::Ime`) when the state changed, and the accessibility tree (`Out::Tree`) when its encoding differs from the last. Selection handles are drawn into the frame (accent colour `rgb(0, 117, 255)`; knob radius 9 CSS px, stem 2 CSS px).

Errors: the output's (the host has gone away).

### `run`

```rust
pub fn run(renderer: &Rc<NativeRenderer>) -> io::Result<()>
```

`serve` over `io::stdin()` and `io::stdout()`. Errors: the output's.

### `run_with`

```rust
pub fn run_with(renderer: &Rc<NativeRenderer>, bridge: &Bridge) -> io::Result<()>
```

`serve_with` over standard input and output.

### `serve`

```rust
pub fn serve(
    renderer: &Rc<NativeRenderer>,
    input: impl Read + Send + 'static,
    output: impl Write,
) -> io::Result<()>
```

`serve_with` with a new `Bridge`.

### `serve_with`

```rust
pub fn serve_with(
    renderer: &Rc<NativeRenderer>,
    mut input: impl Read + Send + 'static,
    output: impl Write,
    bridge: &Bridge,
) -> io::Result<()>
```

Spawns a thread named `surface-input` that reads events with `read_event` and sends them over a channel (any read error or end of input becomes `Closed`). The loop waits for an event until `min(next_timer, renderer.next_wake())`, handles it and every event already queued, runs `timers`, `renderer.tick()` and `flush`. It returns `Ok(())` when the input closes (or `finished()` is true) and the output's error otherwise. Errors from creating the thread are returned too.

## `proto`

Messages between the host and the program. Every message is a 4-byte length, a 1-byte kind and a payload, all numbers big-endian. Byte layouts are in [internals](internals.md#protocol).

### `enum Event`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Resize { width: u32, height: u32, density: f32, insets: [f32; 4] },
    Touch { action: u8, x: f32, y: f32 },
    Key { down: bool, mods: u8, name: String },
    Text(String),
    Compose { text: String, cursor: i32 },
    Delete { before: u32, after: u32 },
    Access { id: u32, action: u8 },
    Scheme { dark: bool },
    Back,
    Visible(bool),
    Clipboard(String),
    Reply(String),
}
```

What the host tells the program. `insets` are top, right, bottom, left in pixels. `Touch.action`: 0 down, 1 move, 2 up, 3 (or any other value) cancelled. `Key.name` is the DOM `key` value. `Compose.cursor` and `Delete` are in UTF-16 units; a negative cursor means none. `Access.action`: 0 click, 1 focus, 2 scroll into view. `Reply` is a JSON envelope `{"id","ok"}`, `{"id","err"}` or `{"event","plugin","data"}`.

### `read_event`

```rust
pub fn read_event(input: &mut impl Read) -> io::Result<Option<Event>>
```

Reads the next event; `Ok(None)` when the pipe closed cleanly before a length (an `UnexpectedEof` while reading the length). Errors (`InvalidData`): length `0` or above `1 << 26` ("a message of that length is not one"), a payload shorter than its kind needs ("a message shorter than its kind needs"), text that is not UTF-8 ("text that is not UTF-8"), an unknown kind ("unknown message kind N"). A stream cut inside the body is the stream's error.

### `encode_event`

```rust
#[must_use]
pub fn encode_event(event: &Event) -> Vec<u8>
```

Encodes an event as the host does, for tests and tools. Short strings (`Key.name`, `Text`, `Compose.text`) are truncated at 65535 bytes (no check for UTF-8 boundaries).

### `enum Out<'a>`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Out<'a> {
    Frame { width: u32, height: u32, rgba: &'a [u8] },
    Keyboard(bool),
    Ime { text: &'a str, cursor: i32, anchor: i32, rect: [f32; 4] },
    Tree(&'a [u8]),
    BackHandled,
    Exit,
    Clipboard(&'a str),
    Call(&'a str),
}
```

What the program tells the host. `Frame.rgba` is `width * height * 4` bytes, RGBA, premultiplied. `Ime`: text of the focused field, caret and the other end of the selection in UTF-16 units, the caret's rectangle (x, y, w, h) in pixels. `Tree` is the output of `encode_tree`. `BackHandled`: the page used the back gesture. `Exit`: the app is finished. `Clipboard`: text for the system clipboard. `Call`: a JSON envelope `{"id","plugin","method","args"}`.

### `write_out`

```rust
pub fn write_out(output: &mut impl Write, message: &Out<'_>) -> io::Result<()>
```

Writes one message and flushes. A frame's pixels go straight to the stream without being copied into a message first. Errors: the stream's.

### `action_of`

```rust
#[must_use]
pub const fn action_of(code: u8) -> Option<AccessAction>
```

`0` is `AccessAction::Click`, `1` `Focus`, `2` `ScrollIntoView`; anything else is `None`.

### `encode_tree`

```rust
#[must_use]
pub fn encode_tree(root: &AccessNode) -> Vec<u8>
```

The accessibility tree as a flat list in document order; layout in [internals](internals.md#the-accessibility-tree).

## `bridge`

### `struct CallError`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub code: String,
    pub message: String,
}
```

Why a call failed: the plugin's error code and message (`unknown_plugin`, `invalid_args`, `permission_denied`, `internal`, ...). `Display` prints `"{code}: {message}"`. When an error envelope lacks `code` it is `internal`; a missing `message` is empty.

### `struct Bridge`

```rust
#[derive(Clone, Default)]
pub struct Bridge { /* Rc<RefCell<State>> */ }
```

The way to the host's plugins. Clones share one state, which belongs to the program's single thread (not `Send`, not `Sync`). `Debug` prints `Bridge { .. }`.

#### `Bridge::new`

```rust
#[must_use]
pub fn new() -> Self
```

#### `Bridge::call`

```rust
pub fn call(
    &self,
    plugin: &str,
    method: &str,
    args: Value,
    then: impl FnOnce(Result<Value, CallError>) + 'static,
)
```

Queues `{"id": <n>, "plugin", "method", "args"}` (ids count from 1) and stores `then`, which runs with the answer when it arrives. A reply with `ok` gives `Ok(value)` (`Null`/default if `ok` is absent); a reply with `err` gives `Err(CallError)`. The closure is removed before it runs, so it may call again.

#### `Bridge::on`

```rust
pub fn on(&self, plugin: &str, event: &str, handler: impl FnMut(&Value) + 'static)
```

Runs `handler` with the event's `data` for each `{"event","plugin","data"}` envelope. Several handlers per `(plugin, event)` are kept in registration order; handlers added while handlers run come after them. There is no way to remove a handler.
