---
title: API reference
parent: quench-desktop-atspi
grand_parent: Crates
nav_order: 2
---

Modules: `conn`, `message`, `service`, `value`, all public. Root re-exports: `Connection`, `Sender`, `Kind`, `Message`, `ROOT_PATH`, `Service`, `Shared`, `Value`. `Change`, `Published`, `conn::connect`, `conn::authenticate_client`, `conn::authenticate_server`, `value::split_signature` are public but only reachable through their modules. Linux only.

## `value`

### `enum Value`

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Byte(u8), Bool(bool), I16(i16), U16(u16), I32(i32), U32(u32), I64(i64), U64(u64), F64(f64),
    Str(String), Path(String), Signature(String),
    Array(String, Vec<Self>),     // element signature, items
    Struct(Vec<Self>),
    DictEntry(Box<Self>, Box<Self>),
    Variant(Box<Self>),
}
```

One value of the D-Bus type system (`y b n q i u x t d s o g a ( ) { } v`). Little-endian only.

Methods:

* `#[must_use] pub fn str(text: &str) -> Self`: `Str`.
* `#[must_use] pub fn path(text: &str) -> Self`: `Path`.
* `#[must_use] pub fn reference(bus_name: &str, path: &str) -> Self`: `(so)`, an object on a bus connection.
* `#[must_use] pub fn signature(&self) -> String`: the type signature.
* `#[must_use] pub fn strings(items: &[&str]) -> Self`: `as`.
* `#[must_use] pub fn as_str(&self) -> Option<&str>`: the string of `s`, `o` or `g`.
* `#[must_use] pub fn as_i64(&self) -> Option<i64>`: the integer of `y n q i u x`; `None` for `t`, `d`, `b` and others.
* `pub fn encode(&self, buf: &mut Vec<u8>)`: appends the wire form; `buf` holds the message so far because alignment counts from its start.
* `pub fn decode(signature: &str, data: &[u8], at: &mut usize) -> io::Result<Self>`: reads one value of the complete type `signature` from `data` at `*at`. Errors (`InvalidData`): short or mis-sized data, strings without NUL or not UTF-8, arrays over `1 << 26` bytes or overrunning their length, a variant that does not hold exactly one value, a dict entry without a key and a value, unknown types, nesting deeper than 32 levels.

### `split_signature`

```rust
pub fn split_signature(signature: &str) -> io::Result<Vec<String>>
```

Splits a signature into its complete types: `a{sv}i(ss)` gives `a{sv}`, `i`, `(ss)`. Errors: unbalanced, unclosed, or unknown signature characters.

## `message`

### `enum Kind`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { MethodCall, MethodReturn, Error, Signal }
```

Wire codes 1, 2, 3, 4.

### `struct Message`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub kind: Kind,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub body: Vec<Value>,
}
```

A message, as sent or received. Fields that a kind does not use are `None`. `serial` is set by `Sender::send`.

* `#[must_use] pub fn call(destination: Option<&str>, path: &str, interface: &str, member: &str, body: Vec<Value>) -> Self`: a method call.
* `#[must_use] pub fn reply(call: &Self, body: Vec<Value>) -> Self`: `MethodReturn` with `reply_serial = call.serial` and `destination = call.sender`.
* `#[must_use] pub fn error(call: &Self, name: &str, text: &str) -> Self`: `Error` with `error_name` and a body of one string.
* `#[must_use] pub fn signal(path: &str, interface: &str, member: &str, body: Vec<Value>) -> Self`: a signal.
* `#[must_use] pub fn encode(&self) -> Vec<u8>`: wire form, little-endian: byte `l`, the type code, flags (`NO_REPLY_EXPECTED` = 1 for signals, otherwise 0), protocol version 1, body length, serial, the header fields array `a(yv)` (path 1, interface 2, member 3, error name 4, reply serial 5, destination 6, sender 7, signature 8), padding to 8, body.
* `pub fn read(stream: &mut impl Read) -> io::Result<Self>`: reads one message. Errors: the stream's; first byte not `l` ("only little-endian messages are supported"); header fields over 65536 bytes or body over 16 MiB ("message too long"); unknown type ("unknown message type"); body not as long as the header says.
* `pub fn write(&self, stream: &mut impl Write) -> io::Result<()>`.
* `pub fn from_bytes(bytes: &[u8]) -> io::Result<Self>`: `read` over a slice, for tests.

## `conn`

### `connect`

```rust
pub fn connect(address: &str) -> io::Result<UnixStream>
```

Connects to a bus address: `unix:path=...` or `unix:abstract=...`; `;`-separated alternatives are tried in order; `%xx` escapes in values are decoded. Errors: no usable address (`InvalidInput`) or the last connection error.

### `authenticate_client`

```rust
pub fn authenticate_client(stream: &mut UnixStream) -> io::Result<()>
```

Sends `\0AUTH EXTERNAL <hex of the decimal uid>\r\n`, expects a line starting `OK` (else `PermissionDenied`, `authentication refused: <line>`), then sends `BEGIN\r\n`. The uid is the owner of `/proc/self`. Lines over 4096 bytes are an error.

### `authenticate_server`

```rust
pub fn authenticate_server(stream: &mut UnixStream) -> io::Result<()>
```

The other end, for a peer that accepts a direct connection: reads the NUL byte and an `AUTH EXTERNAL` line (anything else gets `REJECTED EXTERNAL` and `PermissionDenied`), answers `OK 0123456789abcdef0123456789abcdef`, expects `BEGIN`.

### `struct Connection`

```rust
pub struct Connection {
    // private streams and serial counter
    pub name: String,
}
```

A connection; `name` is the unique name the bus gave (`:1.42`) or what the peer was told to be. `Debug` prints `name`.

* `pub fn new(stream: UnixStream, name: &str) -> io::Result<Self>`: over an authenticated stream (clones it for the writer).
* `pub fn bus(address: &str) -> io::Result<Self>`: connect, authenticate, call `org.freedesktop.DBus.Hello` and store the returned name. Error `Hello gave no name` if the reply has no string.
* `#[must_use] pub fn sender(&self) -> Sender`.
* `pub fn send(&self, message: Message) -> io::Result<u32>`: sends with the next serial and returns it.
* `pub fn recv(&mut self) -> io::Result<Message>`: the next message (timeouts are `WouldBlock`/`TimedOut`).
* `pub fn set_timeout(&self, timeout: Option<Duration>) -> io::Result<()>`: the read timeout of `recv`.
* `pub fn call(&mut self, message: Message) -> io::Result<Message>`: sends and waits for the message whose `reply_serial` equals the serial; other messages that arrive meanwhile are dropped. An `Error` reply becomes `io::Error::other("<error name>: <text>")`.

### `struct Sender`

```rust
#[derive(Clone)]
pub struct Sender { /* shared writer and serial */ }
```

What is needed to send from another thread. `Debug` prints `Sender { .. }`.

* `pub fn send(&self, message: &mut Message) -> io::Result<u32>`: assigns the next serial (starting at 1), writes the message under a lock, returns the serial.

## `service`

### `ROOT_PATH`

```rust
pub const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
```

### `enum Change`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Name(usize, String),
    Child { parent: usize, child: usize, index: usize, added: bool },
    Text { id: usize, offset: usize, removed: String, inserted: String },
    Caret(usize, usize),
    State(usize, &'static str, bool),
}
```

What changed between two published pages. `Text` offsets are in characters; `State`'s name is `checked`, `expanded` or `selected`.

### `struct Published`

```rust
#[derive(Debug)]
pub struct Published {
    pub focus: (Option<usize>, Option<usize>),
    pub changes: Vec<Change>,
}
```

What `Shared::publish` found: the focused node id before and now, and the changes.

### `struct Shared`

State shared between the answering thread and the program. `Debug` prints the bus name.

* `#[must_use] pub fn new(app_name: &str) -> Arc<Self>`.
* `pub fn set_name(&self, name: &str)`: the connection's unique bus name, used in object references.
* `pub fn set_registry_root(&self, name: &str, path: &str)`: the registry's root (the application object's parent). Default `("", "/org/a11y/atspi/null")`.
* `pub fn publish(&self, tree: AccessNode) -> Published`: replaces the snapshot; returns the focus before/now and the changes.
* `pub fn take_edits(&self) -> Vec<(usize, String)>`: the whole texts set in text fields since the last call.
* `pub fn take_actions(&self) -> Vec<(usize, AccessAction)>`.
* `pub fn take_asked(&self) -> bool`: whether any call arrived since the last call (resets).
* `pub fn handle(&self, call: &Message) -> Option<Message>`: answers a method call (`None` if `call` is not a method call or has no path or member). See [AT-SPI coverage](atspi.md).

### `struct Service`

A running service: the answering thread plus what the program shares with it. `Debug` prints `shared`.

* `pub fn start(app_name: &str) -> io::Result<Self>`: session bus (see [Environment](index.md#environment)), then `org.a11y.Bus.GetAddress`, connect to the accessibility bus, `Embed`, serve. Errors: no session bus (`no session bus`), no accessibility bus, or a bus that answers wrongly (`no address`, ...).
* `pub fn start_on(session: Connection, app_name: &str) -> io::Result<Self>`: as `start` with the session bus connection given. Sets 5 s timeouts for the handshake and none afterwards. If the registry answers `Embed` with an `(so)`, it becomes the registry root.
* `#[must_use] pub fn serve(connection: Connection, shared: Arc<Shared>) -> Self`: starts the thread named `atspi` that receives messages and replies (the reply destination is the call's sender); the thread ends when `recv` or sending fails. Panics (`expect`) if the thread cannot be created.
* `#[must_use] pub const fn shared(&self) -> &Arc<Shared>`.
* `pub fn publish(&self, tree: AccessNode)`: `Shared::publish`, then signals (below), then focus signals.
* `#[must_use] pub fn take_edits(&self)`, `take_actions(&self)`, `take_asked(&self)`: as on `Shared`.

Signals sent by `publish` (interface `org.a11y.atspi.Event.Object`, body `(siiv a{sv})`: kind, detail1, detail2, data variant, empty properties):

| Change | Member | Kind | Detail 1 | Detail 2 | Data |
|---|---|---|---|---|---|
| `Name` | `PropertyChange` | `accessible-name` | 0 | 0 | the new name (string) |
| `Child` | `ChildrenChanged` (on the parent) | `add` or `remove` | index | 0 | `(so)` reference of the child |
| `Text` | `TextChanged` (delete first, then insert, if non-empty) | `delete` / `insert` | offset | length in characters | the text |
| `Caret` | `TextCaretMoved` | empty | caret | 0 | `0` |
| `State` | `StateChanged` | `checked` / `expanded` / `selected` | 0 or 1 | 0 | `0` |
| focus moved | `StateChanged` on the old node, then the new | `focused` | 0 / 1 | 0 | `0` |

Signals are sent without error handling (send failures are ignored).
