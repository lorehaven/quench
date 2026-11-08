---
title: Graphics
parent: quench-cli
grand_parent: Crates
nav_order: 6
---

Module `graphics`: images in the terminal. It builds kitty graphics protocol sequences, encodes sixel images, parses what terminals answer to graphics queries, and detects which protocol the terminal speaks. The module itself has no feature gate. Only `detect::detect` and `detect::detect_in` need `#[cfg(all(unix, feature = "tui"))]`, because they put the controlling terminal in raw mode. Everything else is pure: it works on strings and bytes and does no I/O. There is no `unsafe`.

The module is written for a terminal Markdown viewer. Such a viewer shows typeset formulas and pictures inline where the terminal can draw them, and text where it cannot. Inside terminal multiplexers (tmux, screen) no query is ever sent, and there are no graphics unless `QUENCH_GRAPHICS` forces a protocol. A forced protocol is then reported without a cell size, text area or colours. Passing the sequences through a multiplexer is not implemented.

Submodules, in source order: `detect`, `kitty`, `reply`, `sixel`. The module root defines `Rgb`.

## `struct Rgb`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb { pub r: u8, pub g: u8, pub b: u8 }

impl Rgb {
    pub const BLACK: Self;   // (0, 0, 0)
    pub const WHITE: Self;   // (255, 255, 255)
    #[must_use] pub const fn new(r: u8, g: u8, b: u8) -> Self
}
```

A colour with 8 bits per channel. `Default` is black. The sixel encoder, the colour replies and `Capabilities` all use it.

## `detect`

Detection decides which graphics the terminal shows, and finds the cell size and colours to draw with. It works in three steps:

1. `plan` reads only the environment and decides whether to ask the terminal.
2. `probe` sends every query at once and reads the answers.
3. `resolve` combines the plan, the answers and the terminal size into `Capabilities`.

`detect` runs all three on the controlling terminal.

### Constants

| Name | Type | Value |
|---|---|---|
| `DEFAULT_TIMEOUT` | `Duration` | 200 ms |
| `QUERIES` | `&str` | `"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"` `"\x1b[16t"` `"\x1b[14t"` `"\x1b]10;?\x1b\\"` `"\x1b]11;?\x1b\\"` `"\x1b[c"` (concatenated) |

`QUERIES` contains these queries, in this order:

1. The kitty graphics query (`kitty::QUERY`, checked against it at compile time).
2. `CSI 16 t`: the cell size in pixels.
3. `CSI 14 t`: the text area in pixels.
4. `OSC 10 ?` and `OSC 11 ?`: the default foreground and background.
5. The primary device attributes query `CSI c`, last.

Every terminal answers device attributes, and terminals answer in order, so the DA1 reply marks the end of the answers. A terminal ignores any query it does not know.

Internally, reading stops after 64 KiB (`MAX_REPLY`, private). This protects against a terminal that echoes garbage.

### `enum Protocol`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol { Kitty, Sixel }
```

### `struct Capabilities`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub kitty: bool,
    pub sixel: bool,
    pub cell_size_px: Option<(u32, u32)>,   // one cell, (width, height) in pixels
    pub text_area_px: Option<(u32, u32)>,   // the text area, (width, height) in pixels
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
}

impl Capabilities {
    pub const NONE: Self;   // no graphics, nothing measured (equal to Default)
    #[must_use] pub fn from_replies(replies: &Replies, size: Option<(u16, u16)>) -> Self
    #[must_use] pub const fn protocol(&self) -> Option<Protocol>
}
```

* `from_replies(replies, size)` copies the answers. `sixel` is `replies.sixel()`, so device attribute 4 must be present. When the terminal did not report the cell size, it is derived from the text area divided by `size`, given as `(columns, rows)`. A zero dimension in `size`, or a quotient of zero in either direction, leaves the cell size `None`. A reported cell size always wins over a derived one.
* `protocol()` returns `Kitty` when `kitty` is true, otherwise `Sixel` when `sixel` is true, otherwise `None`. Kitty wins when both are available.

### `enum Forced`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forced { None, Kitty, Sixel }

impl Forced {
    #[must_use] pub fn parse(value: &str) -> Option<Self>
}
```

`Forced` holds the value of the environment variable `QUENCH_GRAPHICS`. `parse` accepts `none`, `kitty` or `sixel` in any case, with surrounding white space trimmed (`" Kitty "` is `Kitty`). Any other value returns `None`, and the variable is then ignored. Note that `Forced::None` (no graphics) is different from `Option::None` (no override).

### `struct Environment`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Environment {
    pub term: Option<String>,       // TERM
    pub graphics: Option<String>,   // QUENCH_GRAPHICS
    pub multiplexer: bool,          // inside tmux or screen
    pub terminal: bool,             // stdin and stdout are both terminals
}

impl Environment {
    #[must_use] pub fn from_process() -> Self
}
```

`from_process` reads the following through `std::env::var`, so a value that is not Unicode counts as unset:

* `TERM` and `QUENCH_GRAPHICS`.
* `multiplexer` is true when any of these holds:
  * `TMUX` is set and non-empty;
  * `STY` is set and non-empty;
  * `TERM` starts with `tmux` or `screen`.
* `terminal` is `term::stdin_is_tty() && term::stdout_is_tty()` with `all(unix, feature = "tui")`, and always `false` without it.

### `struct Plan` and `fn plan`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub probe: bool,              // send the queries and read the answers
    pub forced: Option<Forced>,   // what QUENCH_GRAPHICS forces
}

#[must_use]
pub fn plan(environment: &Environment) -> Plan
```

`forced` is `Forced::parse` of `graphics`. `probe` is true only when all of these hold:

* `terminal` is true.
* `TERM` is set, non-empty and not `dumb`.
* `multiplexer` is false. Inside tmux or screen nothing is sent, even with an override: the queries would go to the multiplexer unwrapped.
* `QUENCH_GRAPHICS` is not `none`.

| Environment | `probe` | Result after `resolve` |
|---|---|---|
| stdin or stdout not a terminal | no | `NONE`, or the forced protocol with nothing measured |
| `TERM` unset, empty or `dumb` | no | as above |
| tmux / screen, nothing forced | no | `NONE` |
| tmux / screen, `QUENCH_GRAPHICS=kitty` or `sixel` | no | the forced protocol, with nothing measured: `cell_size_px`, `text_area_px`, `foreground` and `background` are `None` |
| `QUENCH_GRAPHICS=none` | no | `NONE` |
| `QUENCH_GRAPHICS=kitty` or `sixel`, outside a multiplexer | yes (when a terminal and `TERM` allow it) | the forced protocol, with the measured sizes and colours |
| otherwise | yes | what the terminal answered |

### `fn resolve`

```rust
#[must_use]
pub fn resolve(plan: Plan, replies: Option<&Replies>, size: Option<(u16, u16)>) -> Capabilities
```

`resolve` starts from `Capabilities::from_replies(replies, size)`, or from `NONE` when `replies` is `None` (nothing was asked, or reading failed). It then applies the override:

* `Forced::None` replaces everything with `NONE`.
* `Forced::Kitty` sets `kitty = true, sixel = false`.
* `Forced::Sixel` sets `kitty = false, sixel = true`.

Sizes and colours stay as measured, so they are `None` when nothing was asked (inside a multiplexer, or without a terminal). A caller that draws with a forced protocol then picks its own cell size and background.

### `fn probe`

```rust
pub fn probe(
    output: &mut dyn Write,
    input: &mut dyn Read,
    timeout: Duration,
    elapsed: &mut dyn FnMut() -> Duration,
) -> io::Result<Replies>
```

`probe` writes `QUERIES` to `output` and flushes it. It then reads from `input` in chunks of 1024 bytes while `elapsed() < timeout` and fewer than 64 KiB have arrived. After each read that returns bytes, it parses everything received so far, and it stops as soon as a device attributes reply is present. Finally it returns `parse_replies` of everything received. Answers that arrived without the sentinel are kept when the time runs out.

* A read of 0 bytes means "nothing yet", not end of input. A terminal in raw mode with a read timeout returns 0 bytes when nothing came for a tenth of a second. `input` must therefore not block for long, because `elapsed` is checked only between reads.
* A read that returns 0 bytes, or fails with `WouldBlock`, *at once* (by `elapsed`, less than 50 ms after it started) is followed by a sleep of 10 ms. An input at its real end, which returns `Ok(0)` immediately forever, therefore costs about one read per 10 ms until the timeout, not a busy loop. The tty path (a 0 after 100 ms) never sleeps.
* `Interrupted`, `WouldBlock` and `TimedOut` errors are retried. `UnexpectedEof` stops reading without an error.
* Errors: writing or flushing the queries fails, or a read fails in any other way.
* Bytes read after the DA1 reply in the same chunk are consumed and parsed along with the answers.

The clock is injectable, so `probe` can be tested with canned replies and a fake clock (see [Testing](testing.md)).

### `fn detect` and `fn detect_in` (unix + `tui`)

```rust
#[cfg(all(unix, feature = "tui"))]
#[must_use]
pub fn detect(timeout: Duration) -> Capabilities

#[cfg(all(unix, feature = "tui"))]
#[must_use]
pub fn detect_in(environment: &Environment, timeout: Duration) -> Capabilities
```

`detect(timeout)` is `detect_in(&Environment::from_process(), timeout)`. Neither function ever fails: anything that cannot be found out is reported as absent.

`detect_in` works as follows:

1. It calls `plan(environment)`. When `probe` is false, it returns `resolve(plan, None, None)` without sending anything.
2. Otherwise it puts file descriptor 0 into raw mode with `RawMode::enable_with_read_timeout(0, 1)`, which sets `min 0 time 1`, so each read returns after at most a tenth of a second.
3. Before raw mode, it locks `std::io::stdout()`, flushes it and flushes `std::io::stderr()`. The queries are written through that locked standard output, so text printed earlier (even with `print!` and no newline) reaches the terminal first, and no other thread's output lands between the queries. It reads the answers from descriptor 0 through `probe`, timed by `Instant::now()`, then restores the terminal. If stdin is not a terminal, or raw mode or reading fails, the replies count as `None`. The stdout lock is reentrant, so calling `detect` while the same thread holds it does not deadlock.
4. When the terminal reported a text area but no cell size, it calls `term::size()` to derive the cell size.
5. It returns `resolve(plan, replies, size)`.

The total wait is at most about `timeout` plus one read granularity of 100 ms.

Limits that follow from this design:

* **Typing is swallowed.** Keys pressed during the probe (up to about 200 ms with `DEFAULT_TIMEOUT`) are read together with the replies and discarded.
* **Late replies leak.** A reply that comes after the timeout is not read. It stays in the program's input, where a line editor or key reader will see it.
* **No multiplexer passthrough.** Inside tmux or screen nothing is asked. With `QUENCH_GRAPHICS=kitty|sixel` the forced protocol is reported unmeasured, and the image sequences you print later go to the multiplexer unwrapped.

## `kitty`

These functions build sequences for the [kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/). They can send a PNG and show it at the cursor, show an image again by id, and delete it.

Images stay in the terminal under the id they were sent with. A pager that scrolls back to an image can therefore `place` it again instead of sending the PNG a second time. Every command these functions build carries `q=2` (quiet), so the terminal never answers into the program's input.

### Constants

| Name | Type | Value |
|---|---|---|
| `CHUNK` | `usize` | `4096`: the largest base64 payload of one escape sequence |
| `QUERY` | `&str` | `"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"`: a 1×1 RGB image, checked but not stored (`a=q`). A kitty-capable terminal answers `ESC _ G i=31;OK ESC \`. |
| `QUERY_ID` | `u32` | `31` |

### `struct Placement`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Placement {
    pub columns: Option<u32>,   // c=: columns to scale to; the image's own width when None
    pub rows: Option<u32>,      // r=: rows to scale to; its own height when None
    pub keep_cursor: bool,      // C=1: leave the cursor where it was
}

impl Placement {
    pub const NATURAL: Self;                                // own size, cursor moves past (same as Default)
    #[must_use] pub const fn cells(columns: u32, rows: u32) -> Self   // scaled, cursor moves past
}
```

The keys are written in the order `c`, `r`, `C`, and only when set.

### Functions

```rust
#[must_use] pub fn transmit_png(png: &[u8], id: u32, placement: Placement) -> String
#[must_use] pub fn place(id: u32, placement: Placement) -> String
#[must_use] pub fn delete(id: u32) -> String
```

| Function | Output |
|---|---|
| `transmit_png` | Base64 of `png` (`quench_core::base64::STANDARD`) with the keys `a=T,f=100,i={id},q=2` and the placement keys. When the base64 is at most 4096 bytes long, the output is one sequence without `m`. Longer data goes in chunks of at most 4096 bytes: the first sequence carries all keys plus `m=1`, the middle ones carry only `m=1`, and the last carries only `m=0`. The function does not check that `png` is a PNG. |
| `place` | `"\x1b_Ga=p,i={id},q=2{placement keys}\x1b\\"`: shows an image already sent, without its data. |
| `delete` | `"\x1b_Ga=d,d=I,i={id},q=2\x1b\\"`: removes every placement of the image and frees its data in the terminal. |

Exact outputs from the tests:

```rust
use quench_cli::graphics::kitty::{self, Placement};

assert_eq!(kitty::transmit_png(b"abc", 7, Placement::NATURAL), "\x1b_Ga=T,f=100,i=7,q=2;YWJj\x1b\\");
assert_eq!(
    kitty::transmit_png(b"abc", 7, Placement::cells(10, 2)),
    "\x1b_Ga=T,f=100,i=7,q=2,c=10,r=2;YWJj\x1b\\"
);
let rows_only = Placement { rows: Some(1), keep_cursor: true, ..Placement::default() };
assert_eq!(kitty::transmit_png(b"", 1, rows_only), "\x1b_Ga=T,f=100,i=1,q=2,r=1,C=1;\x1b\\");
assert_eq!(kitty::delete(42), "\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
```

## `reply`

These are parsers for terminal answers, written as pure functions over bytes. Malformed input is skipped and never causes a panic. The tests run the parsers on every prefix and suffix of real terminals' replies, and on 2000 pseudo-random inputs.

### `struct Replies`

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Replies {
    pub kitty: bool,                          // the kitty query (id 31) was answered OK
    pub device_attributes: Option<Vec<u16>>,  // CSI ? 62 ; 4 c gives [62, 4]
    pub cell_size_px: Option<(u32, u32)>,     // (width, height) from CSI 6 ; h ; w t
    pub text_area_px: Option<(u32, u32)>,     // (width, height) from CSI 4 ; h ; w t
    pub foreground: Option<Rgb>,              // OSC 10 reply
    pub background: Option<Rgb>,              // OSC 11 reply
}

impl Replies {
    #[must_use] pub fn sixel(&self) -> bool   // device attributes contain 4
}
```

### `fn parse_replies`

```rust
#[must_use]
pub fn parse_replies(bytes: &[u8]) -> Replies
```

`parse_replies` takes everything read back from the terminal. The replies may come in any order, mixed with keys typed in the meantime, and cut off at the end. The function finds the escape sequences and keeps the answers it understands:

* CSI sequences (`ESC [`, parameter and intermediate bytes 0x20–0x3f, final byte 0x40–0x7e) are tried as device attributes, then as a window report.
* OSC sequences (`ESC ]`, ended by BEL, `ESC \` or the 8-bit ST 0x9c) are tried as colours 10 and 11.
* APC sequences (`ESC _`, ended by `ESC \` or 0x9c) count as kitty replies. Only a reply with `i=31` and the message `OK` sets `kitty`.
* DCS sequences (`ESC P`) are skipped. Other bytes are skipped.
* The 8-bit introducers 0x9b (CSI), 0x9d (OSC), 0x9f (APC) and 0x90 (DCS) are read like their 7-bit forms. Sequences found in 8-bit form are converted to the 7-bit form before the single-sequence parsers see them.
* An 8-bit control byte (introducer or ST) counts only at the start, after an ASCII byte, or right after another sequence. So the UTF-8 continuation bytes of typed text (`ě` is `C4 9B`, `Ü` is `C3 9C`) are not taken for controls.

When the same kind of answer appears twice, the later one wins. A string sequence (OSC, APC, DCS) cut short by an `ESC` that does not start `ESC \`, or by CAN (0x18) or SUB (0x1a), is dropped, and the scan goes on at that byte. So a reply the terminal never finished does not hide the replies after it: `\x1b]11;rgb:0000/00\x1b[?62;4c` still gives the DA1 reply. A string sequence still open at the end of the input ends the scan (a lone `ESC` at the very end may still become `ESC \`). An unfinished CSI is skipped.

### Single-sequence parsers

Each takes one complete escape sequence in 7-bit form (`parse_replies` converts 8-bit ones before calling them).

```rust
#[must_use] pub fn parse_device_attributes(sequence: &[u8]) -> Option<Vec<u16>>
#[must_use] pub fn parse_window_report(sequence: &[u8]) -> Option<WindowReport>
#[must_use] pub fn parse_osc_colour(sequence: &[u8]) -> Option<(u16, Rgb)>
#[must_use] pub fn parse_kitty_reply(sequence: &[u8]) -> Option<KittyReply>

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowReport {
    CellSize(u32, u32),   // CSI 6 ; h ; w t, as (width, height)
    TextArea(u32, u32),   // CSI 4 ; h ; w t, as (width, height)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyReply {
    pub id: Option<u32>,   // i=
    pub message: String,   // "OK" or an error such as "ENOENT:..."
}
impl KittyReply {
    #[must_use] pub fn ok(&self) -> bool   // message == "OK"
}
```

| Parser | Accepts | Rejects (`None`) |
|---|---|---|
| `parse_device_attributes` | `ESC [ ? Ps ; ... c`. Parameters are decimal and an empty one is 0 (`\x1b[?62;c` gives `[62, 0]`). | DA2 (`ESC [ >`), non-digits, values over `u16` |
| `parse_window_report` | `ESC [ 6 ; h ; w t` and `ESC [ 4 ; h ; w t`. The reply's order is height then width; the result is `(width, height)`. | other kinds (`8;24;80t`), a wrong parameter count, a zero width or height |
| `parse_osc_colour` | `ESC ] Ps ; rgb:R/G/B`, ended by BEL or `ESC \`. Each channel has 1 to 4 hex digits and is scaled to 8 bits (`f` → 255, `80` → 128, `8080` → 128). `rgba:` with a fourth channel, which is ignored, is accepted too. Returns `(Ps, colour)`. | 5 or more digits, the wrong channel count, `#rrggbb`, no terminator |
| `parse_kitty_reply` | `ESC _ G keys ; message ESC \`. `id` is the first `i=` key that parses. | a reply without `;` (`\x1b_GOK\x1b\\`) |

## `sixel`

`sixel` is an encoder from RGBA pixels to a complete DCS sixel string, for terminals without the kitty protocol. Sixel has indexed colours and no alpha, so the encoder works in four steps:

1. **Blend.** Each partly transparent pixel is blended onto the background the caller passes. This should be the terminal's background, `Capabilities::background`. Fully transparent pixels (alpha 0) are left out, and with `P2=1` they keep what the screen shows.
2. **Round.** Blended colours are rounded to whole percent per channel (101 levels), because sixel palettes are given in percent.
3. **Quantize.** When at most `max_colours` distinct colours remain, the palette is exact, ordered by frequency (most frequent first, ties by colour). Otherwise median cut reduces it.
4. **Write.** The pixels are written in bands of six rows. Each band draws only the colours present in it. Runs of the same sixel longer than three are run-length encoded as `!n`, and empty sixels at the end of a row are left out.

A typeset formula (one colour with antialiased edges) thus needs only a few dozen palette entries and is reproduced exactly.

### Items

```rust
pub const MAX_COLOURS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SixelError {
    SizeMismatch { expected: usize, actual: usize },
}
impl fmt::Display for SixelError   // "expected {expected} bytes of RGBA pixels, got {actual}"
impl std::error::Error for SixelError

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quantized {
    pub width: usize,
    pub height: usize,
    pub palette: Vec<Rgb>,          // at most MAX_COLOURS, each exactly expressible in percent
    pub pixels: Vec<Option<u8>>,    // row by row: palette index, None for a fully transparent pixel
}

#[must_use] pub fn blend(colour: Rgb, alpha: u8, background: Rgb) -> Rgb
#[must_use] pub fn as_shown(colour: Rgb) -> Rgb
pub fn quantize(rgba: &[u8], width: usize, height: usize, background: Rgb, max_colours: usize)
    -> Result<Quantized, SixelError>
pub fn encode(rgba: &[u8], width: usize, height: usize, background: Rgb) -> Result<String, SixelError>
#[must_use] pub fn encode_quantized(image: &Quantized) -> String
```

| Item | Behaviour |
|---|---|
| `blend` | Straight alpha, rounded: `(c·a + b·(255 − a) + 127) / 255` per channel. `blend(WHITE, 128, BLACK)` is `(128, 128, 128)`. |
| `as_shown` | The colour after rounding each channel to a whole percent and back (`(1, 2, 3)` becomes `(0, 3, 3)`). |
| `quantize` | Blends, rounds and reduces as described above. `max_colours` is clamped to `1..=256`. Median cut repeatedly splits the box with the widest channel range at its weighted median, and each box's palette colour is its weighted average. Errors: `SizeMismatch` when `rgba.len() != width × height × 4`. |
| `encode` | `encode_quantized(&quantize(rgba, width, height, background, MAX_COLOURS)?)`. |
| `encode_quantized` | Writes the header `ESC P 0;1;0 q`. `P2=1` makes unset pixels transparent. Then the raster attributes `"1;1;{width};{height}` (square pixels), and one `#n;2;r;g;b` per palette entry, in percent. Bands are separated by `-` and colours within a band by `$`, and the image ends with `ESC \`. Pixel indexes outside the palette are skipped. |

From the tests:

```rust
use quench_cli::graphics::{sixel, Rgb};

// One opaque red pixel.
let out = sixel::encode(&[255, 0, 0, 255], 1, 1, Rgb::BLACK)?;
assert_eq!(out, "\x1bP0;1;0q\"1;1;1;1#0;2;100;0;0#0@\x1b\\");

// 100 white pixels in a row: one run.
let out = sixel::encode(&[255; 400], 100, 1, Rgb::BLACK)?;
assert_eq!(out, "\x1bP0;1;0q\"1;1;100;1#0;2;100;100;100#0!100@\x1b\\");

// A fully transparent image has no palette and no pixels.
assert_eq!(sixel::encode(&[255, 0, 0, 0].repeat(16), 4, 4, Rgb::WHITE)?, "\x1bP0;1;0q\"1;1;4;4\x1b\\");

// The wrong buffer size.
assert!(matches!(
    sixel::encode(&[0; 7], 1, 2, Rgb::BLACK),
    Err(sixel::SixelError::SizeMismatch { expected: 8, actual: 7 })
));
# Ok::<(), sixel::SixelError>(())
```

Black at alpha 255, black at alpha 128 and a transparent pixel on white give a two-entry palette, `#0;2;0;0;0#1;2;50;50;50`, and the third pixel is left out. This was checked by running the function.
