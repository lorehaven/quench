---
title: Style
parent: quench-cli
grand_parent: Crates
nav_order: 3
---

Module `style`: the part of `crossterm::style` the tools use. Always available.

## `enum Color`

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. The sixteen ANSI colours and the SGR code each produces:

| Variant | Code | Variant | Code |
|---|---|---|---|
| `Black` | 30 | `DarkGrey` | 90 |
| `DarkRed` | 31 | `Red` | 91 |
| `DarkGreen` | 32 | `Green` | 92 |
| `DarkYellow` | 33 | `Yellow` | 93 |
| `DarkBlue` | 34 | `Blue` | 94 |
| `DarkMagenta` | 35 | `Magenta` | 95 |
| `DarkCyan` | 36 | `Cyan` | 96 |
| `Grey` | 37 | `White` | 97 |

The plain names are the bright colours, the `Dark` names the normal ones.

## `struct Styled<T>`

`#[derive(Debug, Clone, Copy, PartialEq, Eq)]`. A value with colour, bold, dim and underline settings. Fields are private.

```rust
impl<T> Styled<T> {
    #[must_use] pub fn into_inner(self) -> T
    #[must_use] pub const fn bold(mut self) -> Self
    #[must_use] pub const fn dim(mut self) -> Self
    #[must_use] pub const fn underlined(mut self) -> Self
    #[must_use] pub const fn with(mut self, color: Color) -> Self
    #[must_use] pub const fn black(self) -> Self        // also: dark_grey, grey, white, red, dark_red, green,
                                                        // dark_green, yellow, dark_yellow, blue, dark_blue,
                                                        // magenta, dark_magenta, cyan, dark_cyan
}

impl<T: fmt::Display> fmt::Display for Styled<T>
```

* `with(color)` sets the colour (a later call replaces an earlier one); each named method is `with(Color::X)`.
* `Display`: when no attribute is set, or `NO_COLOR` is set and non-empty, the value is written as is (the formatter's width and precision apply to the value). Otherwise `\x1b[<codes>m<value>\x1b[0m` with the codes joined by `;` in the order **bold (1), dim (2), underline (4), colour**. The formatter's width/padding is not applied around the codes.
* `into_inner` returns the value without styling.

## `trait Stylize`

```rust
pub trait Stylize: fmt::Display + Sized {
    fn styled(self) -> Styled<Self>
    fn bold(self) -> Styled<Self>
    fn dim(self) -> Styled<Self>
    fn underlined(self) -> Styled<Self>
    fn black(self) -> Styled<Self>        // dark_grey, grey, white, red, dark_red, green, dark_green,
                                          // yellow, dark_yellow, blue, dark_blue, magenta, dark_magenta,
                                          // cyan, dark_cyan
}
impl<T: fmt::Display> Stylize for T {}
```

Implemented for every `Display` type, so `"x".bold()`, `42.green()` work. Starts a `Styled`; further calls are the `Styled` methods above.

| Expression | Output (no `NO_COLOR`) |
|---|---|
| `"hi".styled()` | `hi` |
| `"hi".bold()` | `\x1b[1mhi\x1b[0m` |
| `"hi".cyan()` | `\x1b[96mhi\x1b[0m` |
| `"hi".dark_cyan()` | `\x1b[36mhi\x1b[0m` |
| `"hi".bold().underlined().white()` | `\x1b[1;4;97mhi\x1b[0m` |
| `42.green().dim()` | `\x1b[2;92m42\x1b[0m` |

(The values are from the unit test of the module.) The `NO_COLOR` check happens at format time, not at construction.

Methods named `bold`, `red`, etc. exist both on `Stylize` and on `Styled`; on a `Styled` value the inherent (consuming `const`) methods win.
