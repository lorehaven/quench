//! SQLite's variable-length integers: 1 to 9 bytes, big-endian, seven bits a byte with the high bit meaning
//! "more"; the ninth byte, when there is one, carries all eight of its bits.

/// Reads a varint at the start of `buf`: the value and the number of bytes it took. `None` when `buf` ends first.
#[must_use]
pub fn get(buf: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    for (i, &byte) in buf.iter().enumerate().take(8) {
        value = (value << 7) | u64::from(byte & 0x7f);
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
    }
    let last = *buf.get(8)?;
    Some(((value << 8) | u64::from(last), 9))
}

/// The bytes a varint of `value` takes.
#[must_use]
pub const fn len(value: u64) -> usize {
    if value > 0x00ff_ffff_ffff_ffff {
        return 9;
    }
    let mut n = 1;
    let mut v = value >> 7;
    while v != 0 {
        n += 1;
        v >>= 7;
    }
    n
}

/// Appends `value` to `out`.
pub fn put(out: &mut Vec<u8>, value: u64) {
    if value > 0x00ff_ffff_ffff_ffff {
        let mut bytes = [0_u8; 9];
        bytes[8] = (value & 0xff) as u8;
        let mut v = value >> 8;
        for slot in bytes[..8].iter_mut().rev() {
            *slot = ((v & 0x7f) as u8) | 0x80;
            v >>= 7;
        }
        out.extend_from_slice(&bytes);
        return;
    }
    let n = len(value);
    for i in (0..n).rev() {
        let part = ((value >> (7 * i)) & 0x7f) as u8;
        out.push(if i == 0 { part } else { part | 0x80 });
    }
}
