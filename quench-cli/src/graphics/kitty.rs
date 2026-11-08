//! The kitty graphics protocol (<https://sw.kovidgoyal.net/kitty/graphics-protocol/>): escape sequences that
//! send a PNG and show it at the cursor, show an image already sent by its id, and delete it.
//!
//! Images are kept by the terminal under the id they were sent with, so a pager that scrolls back to a
//! formula places it again with [`place`] instead of sending the PNG a second time. Every command is sent
//! in quiet mode (`q=2`), so the terminal never answers into the program's input.

use quench_core::base64::STANDARD;
use std::fmt::Write;

/// The largest base64 payload of one escape sequence; longer data goes in several chunks.
pub const CHUNK: usize = 4096;

/// The query [`detect`](super::detect) sends: a 1×1 RGB image with id 31 that is checked, not stored
/// (`a=q`). A terminal that speaks the protocol answers `ESC _ G i=31;OK ESC \`, others ignore it.
pub const QUERY: &str = "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\";

/// The id [`QUERY`] uses.
pub const QUERY_ID: u32 = 31;

/// Where and how big an image is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Placement {
    /// Columns the image is scaled to (`c=`); its own width in cells when `None`.
    pub columns: Option<u32>,
    /// Rows the image is scaled to (`r=`); its own height in cells when `None`.
    pub rows: Option<u32>,
    /// Leaves the cursor where it was (`C=1`) instead of moving it past the image.
    pub keep_cursor: bool,
}

impl Placement {
    /// The image at its own size, the cursor moved past it.
    pub const NATURAL: Self = Self {
        columns: None,
        rows: None,
        keep_cursor: false,
    };

    /// The image scaled to `columns` × `rows` cells.
    #[must_use]
    pub const fn cells(columns: u32, rows: u32) -> Self {
        Self {
            columns: Some(columns),
            rows: Some(rows),
            keep_cursor: false,
        }
    }

    fn keys(self, out: &mut String) {
        if let Some(columns) = self.columns {
            let _ = write!(out, ",c={columns}");
        }
        if let Some(rows) = self.rows {
            let _ = write!(out, ",r={rows}");
        }
        if self.keep_cursor {
            out.push_str(",C=1");
        }
    }
}

/// Sends the PNG file `png` under `id` and shows it at the cursor (`a=T,f=100`).
///
/// The data is base64 encoded and split into chunks of at most [`CHUNK`] bytes: the first sequence carries
/// the keys and `m=1`, the following ones only `m=1`, the last `m=0`. Data that fits in one chunk is one
/// sequence without `m`.
#[must_use]
pub fn transmit_png(png: &[u8], id: u32, placement: Placement) -> String {
    let data = STANDARD.encode(png);
    let mut keys = format!("a=T,f=100,i={id},q=2");
    placement.keys(&mut keys);
    let mut out = String::with_capacity(data.len() + data.len() / CHUNK * 12 + 64);
    if data.len() <= CHUNK {
        let _ = write!(out, "\x1b_G{keys};{data}\x1b\\");
        return out;
    }
    // Base64 is ASCII, so cutting at any byte offset stays on a character boundary.
    let chunks: Vec<&[u8]> = data.as_bytes().chunks(CHUNK).collect();
    let last = chunks.len() - 1;
    for (n, chunk) in chunks.into_iter().enumerate() {
        let more = u8::from(n != last);
        let chunk = String::from_utf8_lossy(chunk);
        if n == 0 {
            let _ = write!(out, "\x1b_G{keys},m={more};{chunk}\x1b\\");
        } else {
            let _ = write!(out, "\x1b_Gm={more};{chunk}\x1b\\");
        }
    }
    out
}

/// Shows the image already sent under `id` at the cursor again (`a=p`), without sending its data.
#[must_use]
pub fn place(id: u32, placement: Placement) -> String {
    let mut keys = format!("a=p,i={id},q=2");
    placement.keys(&mut keys);
    format!("\x1b_G{keys}\x1b\\")
}

/// Deletes the image `id`: every placement of it, and its data in the terminal (`a=d,d=I`).
#[must_use]
pub fn delete(id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
}
