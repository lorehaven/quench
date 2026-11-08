//! Parsers for what terminals answer to the graphics queries, as pure functions over bytes.
//!
//! [`parse_replies`] takes everything read back from the terminal (replies in any order, possibly mixed
//! with keys typed meanwhile, possibly cut off at the end) and picks out the answers it knows. The
//! per-sequence parsers take one complete escape sequence each, in 7-bit form. Malformed input is skipped,
//! never a panic.
//!
//! The scan knows the 7-bit introducers `ESC [`, `ESC ]`, `ESC _`, `ESC P` and their 8-bit forms (`0x9b`,
//! `0x9d`, `0x9f`, `0x90`), and ends string sequences at BEL (OSC only), `ESC \` or the 8-bit ST `0x9c`. An
//! 8-bit control counts only after an ASCII byte, so the continuation bytes of UTF-8 text typed meanwhile
//! are not taken for one. A string sequence interrupted by an `ESC` that does not start `ESC \` (or by CAN
//! or SUB) is dropped, and the scan goes on at that `ESC`, so the replies after an unfinished one are
//! still found. One still open at the end of the input ends the scan.

use super::Rgb;
use super::kitty::QUERY_ID;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;

/// What the terminal answered, as far as it is understood.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Replies {
    /// The terminal answered the kitty graphics query ([`super::kitty::QUERY`]) with `OK`.
    pub kitty: bool,
    /// The primary device attributes (`CSI ? 62 ; 4 c` gives `[62, 4]`); `None` when not answered.
    pub device_attributes: Option<Vec<u16>>,
    /// The cell size in pixels, `(width, height)`, from `CSI 6 ; h ; w t`.
    pub cell_size_px: Option<(u32, u32)>,
    /// The text area in pixels, `(width, height)`, from `CSI 4 ; h ; w t`.
    pub text_area_px: Option<(u32, u32)>,
    /// The default foreground colour, from the `OSC 10` reply.
    pub foreground: Option<Rgb>,
    /// The default background colour, from the `OSC 11` reply.
    pub background: Option<Rgb>,
}

impl Replies {
    /// Whether the device attributes list sixel graphics (attribute 4).
    #[must_use]
    pub fn sixel(&self) -> bool {
        self.device_attributes
            .as_ref()
            .is_some_and(|attributes| attributes.contains(&4))
    }
}

/// One escape sequence found in the input, in its 7-bit form (`ESC [` …, `ESC ]` … `ESC \`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Sequence {
    /// `ESC [` up to and including the final byte.
    Csi(Vec<u8>),
    /// `ESC ]` up to and including the terminator.
    Osc(Vec<u8>),
    /// `ESC _` up to and including `ESC \`.
    Apc(Vec<u8>),
}

/// The 8-bit form of `ESC [` (CSI).
const C1_CSI: u8 = 0x9b;
/// The 8-bit form of `ESC ]` (OSC).
const C1_OSC: u8 = 0x9d;
/// The 8-bit form of `ESC _` (APC).
const C1_APC: u8 = 0x9f;
/// The 8-bit form of `ESC P` (DCS).
const C1_DCS: u8 = 0x90;
/// The 8-bit form of `ESC \` (ST).
const C1_ST: u8 = 0x9c;
/// CAN and SUB cancel a sequence.
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Whether the byte at `i` is an 8-bit control (`C1`) of its own, not the continuation byte of a UTF-8
/// character typed meanwhile: it is one only after an ASCII byte, at the start, or right after a sequence
/// (`boundary`, where the last one ended).
const fn is_c1(bytes: &[u8], i: usize, c1: u8, boundary: usize) -> bool {
    bytes[i] == c1 && (i == 0 || i == boundary || bytes[i - 1] < 0x80)
}

/// How a string sequence (OSC, APC, DCS) whose body starts at `from` ends.
enum StringEnd {
    /// Terminated: the body ends at `body` (exclusive), the terminator at `end` (exclusive).
    Terminated { body: usize, end: usize },
    /// Cut short by an `ESC` that does not start `ESC \`, or by CAN or SUB, at this index: the sequence is
    /// dropped and scanning goes on from there (a new sequence starts at an `ESC`).
    Aborted(usize),
    /// The input ends inside it.
    Unfinished,
}

/// Where the string sequence whose body starts at `from` ends: after BEL (when `bel` is allowed), `ESC \`
/// or the 8-bit ST.
fn string_end(bytes: &[u8], from: usize, bel: bool) -> StringEnd {
    for i in from..bytes.len() {
        match bytes[i] {
            BEL if bel => {
                return StringEnd::Terminated {
                    body: i,
                    end: i + 1,
                };
            }
            ESC if bytes.get(i + 1) == Some(&b'\\') => {
                return StringEnd::Terminated {
                    body: i,
                    end: i + 2,
                };
            }
            // A lone ESC at the very end may still become `ESC \`.
            ESC if i + 1 == bytes.len() => return StringEnd::Unfinished,
            ESC | CAN | SUB => return StringEnd::Aborted(i),
            _ if is_c1(bytes, i, C1_ST, from) => {
                return StringEnd::Terminated {
                    body: i,
                    end: i + 1,
                };
            }
            _ => {}
        }
    }
    StringEnd::Unfinished
}

/// The kind of sequence that starts at `i`, and where its body starts; `boundary` is where the last
/// sequence ended.
fn introducer(bytes: &[u8], i: usize, boundary: usize) -> Option<(u8, usize)> {
    if bytes[i] == ESC {
        return match bytes.get(i + 1) {
            Some(&kind @ (b'[' | b']' | b'_' | b'P')) => Some((kind, i + 2)),
            _ => None,
        };
    }
    [
        (C1_CSI, b'['),
        (C1_OSC, b']'),
        (C1_APC, b'_'),
        (C1_DCS, b'P'),
    ]
    .into_iter()
    .find(|&(c1, _)| is_c1(bytes, i, c1, boundary))
    .map(|(_, kind)| (kind, i + 1))
}

/// The escape sequences in `bytes`, in order, in 7-bit form; other bytes are skipped. A string sequence
/// cut short by a new `ESC` (a reply the terminal never finished) is dropped and scanning goes on at that
/// `ESC`; only one still open at the end of the input stops the scan.
fn sequences(bytes: &[u8]) -> Vec<Sequence> {
    let mut found = Vec::new();
    let (mut i, mut boundary) = (0, 0);
    while i < bytes.len() {
        let Some((kind, body)) = introducer(bytes, i, boundary) else {
            i += 1;
            continue;
        };
        if kind == b'[' {
            let mut j = body;
            while j < bytes.len() && (0x20..0x40).contains(&bytes[j]) {
                j += 1;
            }
            match bytes.get(j) {
                Some(0x40..=0x7e) => {
                    let mut csi = b"\x1b[".to_vec();
                    csi.extend_from_slice(&bytes[body..=j]);
                    found.push(Sequence::Csi(csi));
                    i = j + 1;
                    boundary = i;
                }
                // Not a CSI after all (or cut off): go on from the next byte.
                _ => i = j.max(i + 1),
            }
            continue;
        }
        match string_end(bytes, body, kind == b']') {
            StringEnd::Terminated {
                body: body_end,
                end,
            } => {
                let mut sequence = vec![ESC, kind];
                sequence.extend_from_slice(&bytes[body..body_end]);
                match (kind, bytes[body_end]) {
                    (b']', BEL) => sequence.push(BEL),
                    _ => sequence.extend_from_slice(b"\x1b\\"),
                }
                match kind {
                    b']' => found.push(Sequence::Osc(sequence)),
                    b'_' => found.push(Sequence::Apc(sequence)),
                    // DCS replies (XTGETTCAP and the like) are not asked for; skipped.
                    _ => {}
                }
                i = end;
                boundary = i;
            }
            StringEnd::Aborted(at) => i = at,
            // Unfinished: nothing after it can be told apart from its body.
            StringEnd::Unfinished => break,
        }
    }
    found
}

/// Everything understood in `bytes` (the terminal's answers, in any order).
#[must_use]
pub fn parse_replies(bytes: &[u8]) -> Replies {
    let mut replies = Replies::default();
    for sequence in sequences(bytes) {
        match sequence {
            Sequence::Csi(csi) => {
                if let Some(attributes) = parse_device_attributes(&csi) {
                    replies.device_attributes = Some(attributes);
                } else if let Some(report) = parse_window_report(&csi) {
                    match report {
                        WindowReport::CellSize(w, h) => replies.cell_size_px = Some((w, h)),
                        WindowReport::TextArea(w, h) => replies.text_area_px = Some((w, h)),
                    }
                }
            }
            Sequence::Osc(osc) => match parse_osc_colour(&osc) {
                Some((10, colour)) => replies.foreground = Some(colour),
                Some((11, colour)) => replies.background = Some(colour),
                _ => {}
            },
            Sequence::Apc(apc) => {
                if let Some(reply) = parse_kitty_reply(&apc)
                    && reply.id == Some(QUERY_ID)
                    && reply.ok()
                {
                    replies.kitty = true;
                }
            }
        }
    }
    replies
}

/// Decimal parameters separated by `;`; an empty parameter is 0. `None` on anything else or overflow.
fn parameters(bytes: &[u8]) -> Option<Vec<u32>> {
    bytes
        .split(|&b| b == b';')
        .map(|part| {
            if part.is_empty() {
                return Some(0);
            }
            if !part.iter().all(u8::is_ascii_digit) {
                return None;
            }
            std::str::from_utf8(part).ok()?.parse().ok()
        })
        .collect()
}

/// The attributes of a primary device attributes reply, `CSI ? Ps ; … c` (4 is sixel).
#[must_use]
pub fn parse_device_attributes(sequence: &[u8]) -> Option<Vec<u16>> {
    let body = sequence.strip_prefix(b"\x1b[?")?.strip_suffix(b"c")?;
    parameters(body)?
        .into_iter()
        .map(|n| u16::try_from(n).ok())
        .collect()
}

/// A window size report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowReport {
    /// `CSI 6 ; h ; w t`: one cell, `(width, height)` in pixels.
    CellSize(u32, u32),
    /// `CSI 4 ; h ; w t`: the text area, `(width, height)` in pixels.
    TextArea(u32, u32),
}

/// A `CSI 6 ; h ; w t` or `CSI 4 ; h ; w t` reply. Zero sizes (a terminal that does not know) are `None`.
#[must_use]
pub fn parse_window_report(sequence: &[u8]) -> Option<WindowReport> {
    let body = sequence.strip_prefix(b"\x1b[")?.strip_suffix(b"t")?;
    let [kind, height, width] = parameters(body)?[..] else {
        return None;
    };
    if width == 0 || height == 0 {
        return None;
    }
    match kind {
        6 => Some(WindowReport::CellSize(width, height)),
        4 => Some(WindowReport::TextArea(width, height)),
        _ => None,
    }
}

/// One channel of an X colour spec: 1 to 4 hex digits, scaled to 8 bits.
fn channel(hex: &[u8]) -> Option<u8> {
    if hex.is_empty() || hex.len() > 4 || !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let value = u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
    let max = (1u32 << (4 * hex.len())) - 1;
    u8::try_from((value * 255 + max / 2) / max).ok()
}

/// An `OSC Ps ; rgb:rrrr/gggg/bbbb` colour reply (BEL or ST terminated): `(Ps, colour)`. `rgba:` with a
/// fourth (ignored) channel is taken too.
#[must_use]
pub fn parse_osc_colour(sequence: &[u8]) -> Option<(u16, Rgb)> {
    let body = sequence.strip_prefix(b"\x1b]")?;
    let body = body
        .strip_suffix(b"\x07")
        .or_else(|| body.strip_suffix(b"\x1b\\"))?;
    let semicolon = body.iter().position(|&b| b == b';')?;
    let code: u16 = std::str::from_utf8(&body[..semicolon]).ok()?.parse().ok()?;
    let spec = &body[semicolon + 1..];
    let (channels, count) = if let Some(rest) = spec.strip_prefix(b"rgb:") {
        (rest, 3)
    } else {
        (spec.strip_prefix(b"rgba:")?, 4)
    };
    let parts: Vec<&[u8]> = channels.split(|&b| b == b'/').collect();
    if parts.len() != count {
        return None;
    }
    let r = channel(parts[0])?;
    let g = channel(parts[1])?;
    let b = channel(parts[2])?;
    if count == 4 {
        channel(parts[3])?;
    }
    Some((code, Rgb::new(r, g, b)))
}

/// A kitty graphics reply, `ESC _ G keys ; message ESC \`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyReply {
    /// The image id (`i=`) the reply is about.
    pub id: Option<u32>,
    /// `OK`, or an error such as `ENOENT:…`.
    pub message: String,
}

impl KittyReply {
    /// Whether the command succeeded.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.message == "OK"
    }
}

/// A kitty graphics protocol reply.
#[must_use]
pub fn parse_kitty_reply(sequence: &[u8]) -> Option<KittyReply> {
    let body = sequence.strip_prefix(b"\x1b_G")?.strip_suffix(b"\x1b\\")?;
    let semicolon = body.iter().position(|&b| b == b';')?;
    let id = body[..semicolon]
        .split(|&b| b == b',')
        .find_map(|key| key.strip_prefix(b"i="))
        .and_then(|value| std::str::from_utf8(value).ok()?.parse().ok());
    let message = String::from_utf8_lossy(&body[semicolon + 1..]).into_owned();
    Some(KittyReply { id, message })
}
