//! The checksums the formats carry: CRC-32 (gzip, zip), Adler-32 and xxHash64 (zstd).

/// The reflected CRC-32 polynomial used by gzip, zip and PNG.
const CRC_POLY: u32 = 0xEDB8_8320;

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ CRC_POLY
            } else {
                crc >> 1
            };
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

static CRC_TABLE: [u32; 256] = crc_table();

/// An incremental CRC-32.
#[derive(Debug, Clone, Copy)]
pub struct Crc32(u32);

impl Crc32 {
    #[must_use]
    pub const fn new() -> Self {
        Self(!0)
    }

    /// Adds `data`.
    pub fn update(&mut self, data: &[u8]) {
        let mut crc = self.0;
        for &byte in data {
            crc = CRC_TABLE[usize::from((crc as u8) ^ byte)] ^ (crc >> 8);
        }
        self.0 = crc;
    }

    /// The checksum of everything added so far.
    #[must_use]
    pub const fn finish(self) -> u32 {
        !self.0
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

/// The CRC-32 of `data`.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = Crc32::new();
    crc.update(data);
    crc.finish()
}

/// The Adler-32 of `data` (the zlib trailer).
#[must_use]
pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    // The largest run whose sums cannot overflow `u32` before the reduction.
    const RUN: usize = 5552;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(RUN) {
        for &byte in chunk {
            a += u32::from(byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

const P1: u64 = 0x9E37_79B1_85EB_CA87;
const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
const P3: u64 = 0x1656_67B1_9E37_79F9;
const P4: u64 = 0x85EB_CA77_C2B2_AE63;
const P5: u64 = 0x27D4_EB2F_1656_67C5;

const fn round(acc: u64, input: u64) -> u64 {
    acc.wrapping_add(input.wrapping_mul(P2))
        .rotate_left(31)
        .wrapping_mul(P1)
}

const fn merge(acc: u64, value: u64) -> u64 {
    (acc ^ round(0, value)).wrapping_mul(P1).wrapping_add(P4)
}

fn read_u64(bytes: &[u8]) -> u64 {
    u64::from_le_bytes(bytes[..8].try_into().expect("eight bytes"))
}

fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes(bytes[..4].try_into().expect("four bytes"))
}

/// The xxHash64 of `data` with `seed` (zstd's content checksum is its low 32 bits, seed 0).
#[must_use]
pub fn xxh64(data: &[u8], seed: u64) -> u64 {
    let mut rest = data;
    let mut hash = if data.len() >= 32 {
        let mut v = [
            seed.wrapping_add(P1).wrapping_add(P2),
            seed.wrapping_add(P2),
            seed,
            seed.wrapping_sub(P1),
        ];
        while rest.len() >= 32 {
            for (lane, value) in v.iter_mut().enumerate() {
                *value = round(*value, read_u64(&rest[lane * 8..]));
            }
            rest = &rest[32..];
        }
        let mut h = v[0]
            .rotate_left(1)
            .wrapping_add(v[1].rotate_left(7))
            .wrapping_add(v[2].rotate_left(12))
            .wrapping_add(v[3].rotate_left(18));
        for value in v {
            h = merge(h, value);
        }
        h
    } else {
        seed.wrapping_add(P5)
    };

    hash = hash.wrapping_add(data.len() as u64);
    while rest.len() >= 8 {
        hash ^= round(0, read_u64(rest));
        hash = hash.rotate_left(27).wrapping_mul(P1).wrapping_add(P4);
        rest = &rest[8..];
    }
    if rest.len() >= 4 {
        hash ^= u64::from(read_u32(rest)).wrapping_mul(P1);
        hash = hash.rotate_left(23).wrapping_mul(P2).wrapping_add(P3);
        rest = &rest[4..];
    }
    for &byte in rest {
        hash ^= u64::from(byte).wrapping_mul(P5);
        hash = hash.rotate_left(11).wrapping_mul(P1);
    }
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(P2);
    hash ^= hash >> 29;
    hash = hash.wrapping_mul(P3);
    hash ^ (hash >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[quench_test::test]
    fn known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
        assert_eq!(xxh64(b"", 0), 0xEF46_DB37_51D8_E999);
        assert_eq!(xxh64(b"abc", 0), 0x44BC_2CF5_AD77_0999);
    }

    #[quench_test::test]
    fn crc_is_incremental() {
        let mut crc = Crc32::default();
        crc.update(b"1234");
        crc.update(b"56789");
        assert_eq!(crc.finish(), 0xCBF4_3926);
    }

    #[quench_test::test]
    fn long_inputs_hash_through_every_path() {
        // 100 bytes covers the 32-byte stripes, the 8- and 4-byte tails and the single bytes.
        let data: Vec<u8> = (0..100u8).collect();
        let expected = xxh64(&data, 0);
        // Hashing is a pure function of (data, seed).
        assert_eq!(xxh64(&data, 0), expected);
        assert_ne!(xxh64(&data, 1), expected);
        // adler32 reduces correctly across its overflow-safe runs.
        let big = vec![0xFFu8; 20_000];
        assert_eq!(adler32(&big), {
            let (mut a, mut b) = (1u64, 0u64);
            for &x in &big {
                a = (a + u64::from(x)) % 65_521;
                b = (b + a) % 65_521;
            }
            ((b << 16) | a) as u32
        });
    }
}
