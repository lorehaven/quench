//! Images in the terminal: the kitty graphics protocol, sixel, and finding out which of them (if any) the
//! terminal speaks.
//!
//! A terminal Markdown viewer shows typeset formulas and pictures inline where the terminal can draw them,
//! and text where it cannot. This module is the terminal side of that:
//!
//! * [`kitty`] builds the escape sequences that send a PNG, show it again by id, and delete it;
//! * [`sixel`] turns RGBA pixels into a sixel image (palette of at most 256 colours, run-length encoded),
//!   tuned for the common formula case of one colour with antialiased alpha;
//! * [`reply`] parses what terminals answer to the queries (device attributes, cell and window size in
//!   pixels, foreground and background colours, the kitty graphics query);
//! * [`detect`] sends those queries and collects the answers. The read loop works on any reader and
//!   writer, so it is tested with canned replies; [`detect::detect`] (feature `tui`, Unix) runs it on the
//!   controlling terminal in raw mode.
//!
//! Everything but [`detect::detect`] is pure and available without the `tui` feature. Inside terminal
//! multiplexers (tmux, screen) no query is ever sent (passing the sequences through them is not done
//! here): there are no graphics unless `QUENCH_GRAPHICS` forces a protocol, which is then reported
//! without a cell size or colours.

pub mod detect;
pub mod kitty;
pub mod reply;
pub mod sixel;

/// A colour with 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self::new(0, 0, 0);
    pub const WHITE: Self = Self::new(255, 255, 255);

    #[must_use]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}
