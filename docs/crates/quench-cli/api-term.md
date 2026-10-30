---
title: Keys, term and tty
parent: quench-cli
grand_parent: Crates
nav_order: 4
---

Modules `keys`, `term` and `tty`: `#[cfg(all(unix, feature = "tui"))]`. They replace what the tools used `crossterm` and `rpassword` for. The terminal is configured by running the system `stty` on a handle to the file descriptor (cost: one process per switch), so `stty` must be on `PATH`.

## `tty`

### `struct RawMode`

```rust
#[derive(Debug)]
pub struct RawMode { /* tty: File, saved: String */ }

impl RawMode {
    pub fn enable(fd: i32) -> io::Result<Option<Self>>
    pub fn enable_keeping_input(fd: i32) -> io::Result<Option<Self>>
}
impl Drop for RawMode   // restores the saved attributes (errors ignored)
```

`enable(fd)`: `Ok(None)` when `fd` is not a terminal (or not open; negative values and nonexistent descriptors count as not a terminal). Otherwise saves the attributes (`stty -g`) and runs `stty -echo -icanon -isig -iexten -ixon -icrnl -inlcr -istrip min 1 time 0`: no echo, no line buffering, no signals from the keyboard (Ctrl-C arrives as byte 3), no CR translation. Dropping the value restores the saved attributes. Errors: `stty` cannot be run or refuses the terminal.

`enable_keeping_input(fd)` is the same function (`stty` does not flush input in either case); the name stays for callers that poll for keys between work.

File descriptors 0, 1, 2 are handled through the standard streams; `n > 2` is opened via `/dev/fd/n`.

## `term`

### Constants

| Name | Value | Meaning |
|---|---|---|
| `ENTER_ALTERNATE_SCREEN` | `"\x1b[?1049h"` | switch to the alternate screen buffer |
| `LEAVE_ALTERNATE_SCREEN` | `"\x1b[?1049l"` | back to the normal screen |
| `CLEAR_ALL` | `"\x1b[2J"` | erase the whole screen (cursor stays) |
| `CLEAR_LINE` | `"\x1b[2K"` | erase the cursor's line |
| `HIDE_CURSOR` | `"\x1b[?25l"` | |
| `SHOW_CURSOR` | `"\x1b[?25h"` | |

### Functions

```rust
#[must_use] pub fn move_to(column: u16, row: u16) -> String
pub fn clear_screen(out: &mut impl Write) -> io::Result<()>
#[must_use] pub fn is_tty(fd: i32) -> bool
#[must_use] pub fn stdin_is_tty() -> bool
#[must_use] pub fn stdout_is_tty() -> bool
pub fn size_of(fd: i32) -> io::Result<(u16, u16)>
pub fn size() -> io::Result<(u16, u16)>
pub fn read_secret(fd: i32, prompt: &str) -> io::Result<String>
pub fn prompt_password(prompt: &str) -> io::Result<String>
```

| Function | Behaviour |
|---|---|
| `move_to(column, row)` | `"\x1b[{row+1};{column+1}H"`; both 0-based. `move_to(9, 4)` is `\x1b[5;10H`. |
| `clear_screen(out)` | writes `CLEAR_ALL` and `move_to(0, 0)` and flushes. |
| `is_tty(fd)` | whether the descriptor is a terminal; `false` for invalid descriptors. |
| `stdin_is_tty()`, `stdout_is_tty()` | `is_tty(0)`, `is_tty(1)`. |
| `size_of(fd)` | `(columns, rows)` from `stty size`. Errors: "not a terminal"; "the terminal reports no size" (unparsable or a zero dimension). |
| `size()` | `size_of(1)`, else `size_of(2)`, else `size_of(0)`. Error when none works. |
| `read_secret(fd, prompt)` | writes `prompt` to **standard error**; turns echo off (`stty -echo -echoe -echok echonl`) if `fd` is a terminal; reads bytes up to a newline unbuffered straight from the descriptor; restores the terminal; writes a newline to stderr if it was a terminal. The trailing `\n` (and one `\r`) is not in the result. A descriptor that is not a terminal is read as it is. Errors: no such descriptor ("no such descriptor"), `stty` failures, read errors, input not UTF-8 (`InvalidData`, "input is not UTF-8"). End of input returns what was read (possibly empty). |
| `prompt_password(prompt)` | `read_secret(0, prompt)`. |

## `keys`

Key presses from a terminal in raw mode. Pair it with `RawMode`; there are no key-release events, every event is a press.

### Types

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char), Enter, Esc, Backspace, Tab, BackTab,
    Left, Right, Up, Down, Home, End, PageUp, PageDown, Insert, Delete,
    F(u8),            // F1 to F12
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers { pub ctrl: bool, pub alt: bool, pub shift: bool }
impl Modifiers { pub const NONE: Self }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent { pub code: KeyCode, pub modifiers: Modifiers }
impl From<KeyCode> for KeyEvent        // no modifiers

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parsed {
    Key(KeyEvent, usize),   // a key, and how many bytes it used
    Incomplete,             // the bytes so far are the start of a longer sequence
    Skip(usize),            // bytes that mean nothing; skip this many
}
```

### `fn parse_key`

```rust
#[must_use]
pub fn parse_key(bytes: &[u8]) -> Parsed
```

Decodes one key from the start of `bytes`. Rules (from the code):

* empty input: `Incomplete`.
* `\r` or `\n`: Enter; `\t`: Tab; 0x08 or 0x7f: Backspace.
* 0x00: `Char(' ')` with ctrl. 0x01..=0x1a: `Char('a'..='z')` with ctrl (Ctrl-A is `Char('a')`+ctrl; Ctrl-H/I/J/M are caught earlier as Backspace/Tab/Enter). 0x1c..=0x1f: `Skip(1)`.
* printable ASCII and UTF-8 sequences (2-4 bytes): `Char(c)`, `shift` set when `c.is_uppercase()`; an incomplete multi-byte sequence is `Incomplete`; an invalid lead byte is `Skip(1)`.
* `ESC` alone: `Incomplete` (the reader decides after a wait that it is the Escape key). `ESC ESC`: `Esc` using 1 byte.
* `ESC [ ...` (CSI): final `A B C D` arrows, `H` Home, `F` End, `P Q R S` F1-F4, `Z` BackTab (shift set), and `~` with the first parameter: 1 or 7 Home, 2 Insert, 3 Delete, 4 or 8 End, 5 PageUp, 6 PageDown, 11-15 F1-F5, 17-21 F6-F10, 23-24 F11-F12. The second parameter is the xterm modifier: `1 + (shift=1, alt=2, ctrl=4)`. Unknown final bytes: `Skip`. A non-final control byte inside the sequence skips the bytes so far.
* `ESC O x` (SS3): arrows, Home (`H`), End (`F`), F1-F4 (`P`-`S`); unknown: `Skip(3)`.
* `ESC` followed by anything else: the following key with `alt` set (`ESC x` is Alt-x), consuming one more byte.

### `struct Keys`

```rust
#[derive(Debug)]
pub struct Keys { /* receiver, pending bytes, ended */ }

impl Keys {
    #[must_use] pub fn stdin() -> Self
    #[must_use] pub fn on_fd(fd: i32) -> Self
    pub fn poll(&mut self, timeout: Duration) -> io::Result<bool>
    pub fn read(&mut self) -> io::Result<KeyEvent>
}
```

`on_fd` spawns a thread that reads the descriptor in 64-byte chunks and sends them over a channel, so `poll` can wait with a timeout without `poll(2)`. The thread reads until the descriptor ends: **a program that stops using a `Keys` and goes on reading the same descriptor itself loses the next chunk of input to it.** If the descriptor does not exist, the first wait fails with "no such descriptor".

* `poll(timeout)`: `Ok(true)` when a key press is ready within `timeout` (zero = check without waiting); bytes that mean nothing are dropped meanwhile. A lone `ESC` becomes the Escape key after a 30 ms wait (`ESC_WAIT`) or when the timeout is up. `Ok(false)` on timeout.
* `read()`: waits (up to an hour per attempt, repeatedly) for the next key. A lone ESC followed by 30 ms of silence is `Esc`; an unfinished sequence is dropped after 30 ms rather than wedging the reader.
* Errors: after the input ends (`read` returned 0 bytes, or a read error), every later wait fails the same way; end of input is `io::ErrorKind::UnexpectedEof` ("the terminal input has ended"). `poll` documents "when the descriptor cannot be polled".

```rust,no_run
use quench_cli::keys::{KeyCode, Keys};
use quench_cli::tty::RawMode;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    let _raw = RawMode::enable(0)?;      // None when stdin is not a terminal
    let mut keys = Keys::stdin();
    loop {
        if keys.poll(Duration::from_millis(100))? {
            let key = keys.read()?;
            if key.code == KeyCode::Char('q') { break; }
        }
        // ... draw ...
    }
    Ok(())
}
```
