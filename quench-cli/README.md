# quench-cli
CLI and REPL toolkit with a unified toolbox-style terminal UI.

## What it is
`quench-cli` is the terminal side of the command-line tools: coloured text, banners and status lines, a REPL with a line editor and history, key events, raw mode, terminal size and a no-echo password prompt. It replaces `crossterm`, `rustyline` and `rpassword`, with no `libc` and no `unsafe`: raw mode and size go through the system `stty`.

It pairs with `quench-args`, which parses a list of words, so a REPL can reuse a `Parser` for each line.

## Use cases
- Print banners and status lines in a tool's output.
- Colour fragments of a line with `Stylize` (honours `NO_COLOR`).
- Check that external binaries exist with `require_binary`.
- Run an interactive REPL with line editing and history.

## Example
```rust
use quench_cli::prelude::*;

fn main() -> std::io::Result<()> {
    print_box_banner("Riveter", "interactive REPL");
    let mut env = String::from("default");
    repl_run(repl_prompt("riveter", &env), |line| {
        match line {
            "quit" | "exit" => return ReplControl::Exit,
            other if other.starts_with("env ") => env = other[4..].to_string(),
            other => print_status(Tone::Warn, "?", other),
        }
        ReplControl::Continue(repl_prompt("riveter", &env))
    })
}
```

## Terminal graphics
The `graphics` module puts images in the terminal, for viewers that show typeset formulas and pictures inline:

- `graphics::kitty`: the [kitty graphics protocol](https://sw.kovidgoyal.net/kitty/graphics-protocol/). `transmit_png` sends a PNG under an id and shows it at the cursor (base64 in chunks of 4096 bytes, quiet mode, optional size in cells); `place` shows an image already sent again, `delete` frees it.
- `graphics::sixel`: RGBA pixels to a sixel image. Partly transparent pixels are blended onto the given background, fully transparent ones are left out (`P2=1`); colours are reduced to what sixel's percent palette can express and, when more than 256 remain, median cut. A formula (one colour, antialiased) comes out exact with a few dozen palette entries.
- `graphics::reply`: parsers for the terminal's answers (device attributes, `CSI 6/4 ; h ; w t`, `OSC 10/11` colours, the kitty query reply), in 7-bit or 8-bit form; a reply the terminal never finished does not hide the ones after it.
- `graphics::detect`: `detect(DEFAULT_TIMEOUT)` (feature `tui`) asks the terminal in raw mode for kitty and sixel support, the cell size in pixels and the foreground and background colours, waiting for the device attributes reply (always answered last) or the timeout, about 200 ms. The read loop `probe` takes any reader and writer, so it is tested with canned replies.

```rust,ignore
use quench_cli::graphics::detect::{Protocol, detect, DEFAULT_TIMEOUT};
use quench_cli::graphics::{kitty, sixel, Rgb};

let caps = detect(DEFAULT_TIMEOUT);
match caps.protocol() {
    Some(Protocol::Kitty) => print!("{}", kitty::transmit_png(&png, 1, kitty::Placement::cells(12, 1))),
    Some(Protocol::Sixel) => print!("{}", sixel::encode(&rgba, w, h, caps.background.unwrap_or(Rgb::BLACK))?),
    None => print!("{text}"),
}
```

Nothing is sent when standard input or output is not a terminal or `TERM` is `dumb`. Inside tmux or screen nothing is ever sent (passthrough is not supported) and no graphics are reported. `QUENCH_GRAPHICS=none|kitty|sixel` forces the answer: `none` sends nothing, `kitty` and `sixel` still measure sizes and colours, except inside tmux or screen, where the forced protocol is reported without a cell size or colours. Pending `print!` output is flushed before the queries. A terminal that answers after the timeout leaves its reply in the program's input.

## Features
- `repl` (default): the `editor` and `repl` modules; implies `tui`.
- `tui`: key events, terminal size, secret prompt, screen sequences, `RawMode` and graphics detection (Unix only). The graphics encoders and parsers need no feature.

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-cli/
