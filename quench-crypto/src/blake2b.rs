//! `BLAKE2b` (RFC 7693), unkeyed, output 1 to 64 bytes. Argon2 is built on it.

// The names are the RFC's (`a`..`d`, `m`, `v`, `h`).
#![allow(clippy::many_single_char_names)]

const IV: [u64; 8] = [
    0x6a09_e667_f3bc_c908,
    0xbb67_ae85_84ca_a73b,
    0x3c6e_f372_fe94_f82b,
    0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1,
    0x9b05_688c_2b3e_6c1f,
    0x1f83_d9ab_fb41_bd6b,
    0x5be0_cd19_137e_2179,
];

const SIGMA: [[usize; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

const BLOCK: usize = 128;

const fn mix(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

fn compress(h: &mut [u64; 8], block: &[u8], counter: u128, last: bool) {
    let mut m = [0u64; 16];
    let (chunks, _) = block.as_chunks::<8>();
    for (word, bytes) in m.iter_mut().zip(chunks) {
        *word = u64::from_le_bytes(*bytes);
    }
    let mut v = [0u64; 16];
    v[..8].copy_from_slice(h);
    v[8..].copy_from_slice(&IV);
    v[12] ^= u64::try_from(counter & u128::from(u64::MAX)).unwrap_or(0);
    v[13] ^= u64::try_from(counter >> 64).unwrap_or(0);
    if last {
        v[14] = !v[14];
    }
    for round in 0..12 {
        let s = &SIGMA[round % 10];
        mix(&mut v, 0, 4, 8, 12, m[s[0]], m[s[1]]);
        mix(&mut v, 1, 5, 9, 13, m[s[2]], m[s[3]]);
        mix(&mut v, 2, 6, 10, 14, m[s[4]], m[s[5]]);
        mix(&mut v, 3, 7, 11, 15, m[s[6]], m[s[7]]);
        mix(&mut v, 0, 5, 10, 15, m[s[8]], m[s[9]]);
        mix(&mut v, 1, 6, 11, 12, m[s[10]], m[s[11]]);
        mix(&mut v, 2, 7, 8, 13, m[s[12]], m[s[13]]);
        mix(&mut v, 3, 4, 9, 14, m[s[14]], m[s[15]]);
    }
    for i in 0..8 {
        h[i] ^= v[i] ^ v[i + 8];
    }
}

/// The `BLAKE2b` digest of `data`, `out_len` bytes long (clamped to `1..=64`).
#[must_use]
pub fn blake2b(out_len: usize, data: &[u8]) -> Vec<u8> {
    let out_len = out_len.clamp(1, 64);
    let mut h = IV;
    // parameter block word 0: digest length, no key, fanout 1, depth 1
    h[0] ^= 0x0101_0000 ^ u64::try_from(out_len).unwrap_or(64);

    let mut offset = 0;
    // every block but the last is compressed as it fills; the last one is
    // padded and flagged (an empty input still compresses one zero block)
    while data.len() - offset > BLOCK {
        compress(
            &mut h,
            &data[offset..offset + BLOCK],
            (offset + BLOCK) as u128,
            false,
        );
        offset += BLOCK;
    }
    let mut last = [0u8; BLOCK];
    last[..data.len() - offset].copy_from_slice(&data[offset..]);
    compress(&mut h, &last, data.len() as u128, true);

    h.iter()
        .flat_map(|word| word.to_le_bytes())
        .take(out_len)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    // RFC 7693 appendix A and the reference test vectors.
    #[quench_test::test]
    fn known_digests() {
        assert_eq!(
            hex::encode(blake2b(64, b"abc")),
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );
        assert_eq!(
            hex::encode(blake2b(64, b"")),
            "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce"
        );
        assert_eq!(
            hex::encode(blake2b(32, b"")),
            "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8"
        );
    }

    #[quench_test::test]
    fn lengths_and_block_boundaries() {
        for out in [1, 20, 32, 48, 64] {
            assert_eq!(blake2b(out, b"x").len(), out);
        }
        assert_eq!(blake2b(0, b"x").len(), 1);
        assert_eq!(blake2b(200, b"x").len(), 64);
        // a shorter digest is a different hash, not a truncation
        assert_ne!(blake2b(32, b"abc"), blake2b(64, b"abc")[..32]);
        // inputs around the 128-byte block size all hash and all differ
        let mut seen = std::collections::HashSet::new();
        for len in [0, 1, 127, 128, 129, 255, 256, 257, 1000] {
            assert!(seen.insert(blake2b(32, &vec![7u8; len])), "len {len}");
        }
    }
}
