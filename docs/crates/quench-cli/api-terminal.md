---
title: Terminal and prelude
parent: quench-cli
grand_parent: Crates
nav_order: 5
---

Modules `prelude`, `terminal` and `require`. No feature gates except where stated. No `unsafe`.

## `prelude`

```rust
#[cfg(feature = "repl")]
pub use crate::repl::{ReplControl, run as repl_run};
pub use crate::require::{MissingBinary, require_binary};
pub use crate::terminal::{
    BLUE, BOLD, CYAN, DIM, GREEN, RESET, SEP, SEP_THIN, Tone, WHITE, YELLOW, print_box_banner,
    print_line, print_status, repl_prompt,
};
```

Not in the prelude, but public in `terminal`: `print_component_preview`, `print_error_line`, `print_inline`. `Stylize` is in `quench_cli::style`, not in the prelude.

## `terminal`

Colour is applied by an internal `paint(codes, text)` that wraps text in `\x1b[<codes>m ... \x1b[0m`, or returns the plain text when the environment variable `NO_COLOR` is set to a non-empty value. The constants below are raw sequences and are **not** affected by `NO_COLOR`.

### `enum Tone`

```rust
#[derive(Debug)]
pub enum Tone { Info, Success, Warn, Error }
```

Not `Clone`/`Copy` (kept by-value in `print_status` for semver reasons, per a comment in the code). Label colours used by `print_status`: Info `1;96`, Success `1;92`, Warn `1;93`, Error `1;91`.

### Constants

| Name | Value |
|---|---|
| `RESET` | `"\x1b[0m"` |
| `BOLD` | `"\x1b[1m"` |
| `DIM` | `"\x1b[2m"` |
| `CYAN` | `"\x1b[36m"` |
| `BLUE` | `"\x1b[34m"` |
| `GREEN` | `"\x1b[32m"` |
| `YELLOW` | `"\x1b[33m"` |
| `WHITE` | `"\x1b[37m"` |
| `SEP` | 56 characters `─` (U+2500) |
| `SEP_THIN` | 56 characters `┄` (U+2504) |

All are `pub const X: &str`.

### Functions

```rust
pub fn print_box_banner(title: &str, subtitle: &str)
pub fn print_status(tone: Tone, label: &str, message: &str)
pub fn print_component_preview(name: &str, description: &str)
#[must_use] pub fn repl_prompt(app: &str, context: &str) -> String
pub fn print_line(message: impl AsRef<str>)
pub fn print_error_line(message: impl AsRef<str>)
pub fn print_inline(message: impl AsRef<str>)
```

| Function | Output |
|---|---|
| `print_box_banner` | four lines to stdout: top border `┌` + 45 `─` + `┐` in cyan (`36`); `│ {title:<43} │` in bold bright white (`1;97`); `│ {subtitle:<43} │` in dark grey (`90`); bottom border. Text longer than 43 characters overflows the box (`{:<43}` pads, does not truncate). |
| `print_status` | `"{label} {message}"` + newline to stdout; label in the tone colour (bold), message in bright white (`97`). |
| `print_component_preview` | `"[name] description"` + newline; `[name]` in `1;96`, description in `97`. |
| `repl_prompt` | returns `"{app}({context})> "` with `app` in `1;96` and `context` in dark grey `90` (plain: `app(context)> `). |
| `print_line` | `println!` of the text. |
| `print_error_line` | `eprintln!` of the text. |
| `print_inline` | `print!` without a newline, then flushes stdout (flush errors ignored). |

Example:

```rust
use quench_cli::terminal::{print_inline, print_line, print_status, Tone};
print_line("starting");
print_inline("working... ");
print_status(Tone::Warn, "WARN", "slow disk");
```

## `require` {#require}

A single up-front check for external binaries a tool will shell out to.

```rust
pub struct MissingBinary { /* private: name, hint */ }
impl fmt::Display for MissingBinary   // "`{name}` not found on PATH. {hint}"
impl fmt::Debug for MissingBinary     // same text as Display
impl std::error::Error for MissingBinary

pub fn require_binary(name: &str, hint: &str) -> Result<(), MissingBinary>
```

`require_binary` is `Ok(())` when `name` is an executable file in a `PATH` directory. A `name` that contains the platform path separator is checked directly as a path. On Unix "executable" means a regular file with any execute bit; on non-Unix any file (and `.exe` is tried on Windows). If `PATH` is unset the result is an error. `Debug` equals `Display` on purpose, so that `fn main() -> Result<(), Box<dyn Error>>` prints one line. There is no `#[must_use]` on the function itself (the `Result` is must-use).

```rust
use quench_cli::require::require_binary;
assert!(require_binary("sh", "needed for shelling out").is_ok());
let err = require_binary("no-such-tool-xyz", "install it first").unwrap_err();
assert_eq!(err.to_string(), "`no-such-tool-xyz` not found on PATH. install it first");
```
