//! X25519 key agreement (RFC 7748), for key pairs that are stored and reloaded.
//!
//! `ring` only offers X25519 with single-use ephemeral keys; a long-lived identity (an `age` key, say) has
//! to be loaded from bytes, so the scalar multiplication is here. It works on five 51-bit limbs with a
//! Montgomery ladder whose control flow and memory access do not depend on the secret scalar.

const MASK: u64 = (1 << 51) - 1;

/// A field element modulo 2^255 - 19 in radix 2^51, limbs kept under 2^52.
#[derive(Clone, Copy)]
struct Fe([u64; 5]);

const ZERO: Fe = Fe([0; 5]);
const ONE: Fe = Fe([1, 0, 0, 0, 0]);

impl Fe {
    fn from_bytes(bytes: &[u8; 32]) -> Self {
        let load = |at: usize| {
            let mut word = [0u8; 8];
            let end = (at + 8).min(32);
            word[..end - at].copy_from_slice(&bytes[at..end]);
            u64::from_le_bytes(word)
        };
        // Bit 255 is ignored, as RFC 7748 says for u-coordinates.
        Self([
            load(0) & MASK,
            (load(6) >> 3) & MASK,
            (load(12) >> 6) & MASK,
            (load(19) >> 1) & MASK,
            (load(24) >> 12) & MASK,
        ])
    }

    /// Carry so every limb is below 2^51 plus a small excess.
    const fn carry(mut self) -> Self {
        let l = &mut self.0;
        let mut c = l[0] >> 51;
        l[0] &= MASK;
        l[1] += c;
        c = l[1] >> 51;
        l[1] &= MASK;
        l[2] += c;
        c = l[2] >> 51;
        l[2] &= MASK;
        l[3] += c;
        c = l[3] >> 51;
        l[3] &= MASK;
        l[4] += c;
        c = l[4] >> 51;
        l[4] &= MASK;
        l[0] += c * 19;
        self
    }

    /// The canonical 32-byte little-endian form (fully reduced).
    fn to_bytes(self) -> [u8; 32] {
        let mut h = self.carry().carry().0;
        // Subtract p if h >= p: q is 1 exactly when h + 19 overflows 2^255.
        let mut q = (h[0] + 19) >> 51;
        q = (h[1] + q) >> 51;
        q = (h[2] + q) >> 51;
        q = (h[3] + q) >> 51;
        q = (h[4] + q) >> 51;
        h[0] += 19 * q;
        let mut c = h[0] >> 51;
        h[0] &= MASK;
        h[1] += c;
        c = h[1] >> 51;
        h[1] &= MASK;
        h[2] += c;
        c = h[2] >> 51;
        h[2] &= MASK;
        h[3] += c;
        c = h[3] >> 51;
        h[3] &= MASK;
        h[4] += c;
        h[4] &= MASK;

        let mut out = [0u8; 32];
        let mut acc: u128 = 0;
        let mut bits = 0;
        let mut at = 0;
        for limb in h {
            acc |= u128::from(limb) << bits;
            bits += 51;
            while bits >= 8 && at < 32 {
                out[at] = (acc & 0xff) as u8;
                acc >>= 8;
                bits -= 8;
                at += 1;
            }
        }
        if at < 32 {
            out[at] = (acc & 0xff) as u8;
        }
        out
    }

    fn add(self, other: Self) -> Self {
        let mut r = self.0;
        for (limb, extra) in r.iter_mut().zip(other.0) {
            *limb += extra;
        }
        Self(r).carry()
    }

    fn sub(self, other: Self) -> Self {
        // Add 16p first so no limb goes negative.
        const P16: [u64; 5] = [
            36_028_797_018_963_664,
            36_028_797_018_963_952,
            36_028_797_018_963_952,
            36_028_797_018_963_952,
            36_028_797_018_963_952,
        ];
        let mut r = self.0;
        for ((limb, bias), minus) in r.iter_mut().zip(P16).zip(other.0) {
            *limb = *limb + bias - minus;
        }
        Self(r).carry()
    }

    fn mul(self, other: Self) -> Self {
        let a = self.0.map(u128::from);
        let b = other.0.map(u128::from);
        let b19 = [b[1] * 19, b[2] * 19, b[3] * 19, b[4] * 19];
        let c0 = a[0] * b[0] + a[1] * b19[3] + a[2] * b19[2] + a[3] * b19[1] + a[4] * b19[0];
        let c1 = a[0] * b[1] + a[1] * b[0] + a[2] * b19[3] + a[3] * b19[2] + a[4] * b19[1];
        let c2 = a[0] * b[2] + a[1] * b[1] + a[2] * b[0] + a[3] * b19[3] + a[4] * b19[2];
        let c3 = a[0] * b[3] + a[1] * b[2] + a[2] * b[1] + a[3] * b[0] + a[4] * b19[3];
        let c4 = a[0] * b[4] + a[1] * b[3] + a[2] * b[2] + a[3] * b[1] + a[4] * b[0];
        Self::reduce_wide([c0, c1, c2, c3, c4])
    }

    fn square(self) -> Self {
        self.mul(self)
    }

    fn mul_small(self, k: u64) -> Self {
        let a = self.0.map(u128::from);
        let k = u128::from(k);
        Self::reduce_wide([a[0] * k, a[1] * k, a[2] * k, a[3] * k, a[4] * k])
    }

    fn reduce_wide(mut c: [u128; 5]) -> Self {
        let mask = u128::from(MASK);
        c[1] += c[0] >> 51;
        c[0] &= mask;
        c[2] += c[1] >> 51;
        c[1] &= mask;
        c[3] += c[2] >> 51;
        c[2] &= mask;
        c[4] += c[3] >> 51;
        c[3] &= mask;
        let carry = c[4] >> 51;
        c[4] &= mask;
        c[0] += carry * 19;
        c[1] += c[0] >> 51;
        c[0] &= mask;
        // Every limb is below 2^52 after the carries above, so the narrowing loses nothing.
        #[allow(clippy::cast_possible_truncation)]
        let narrow = |limb: u128| limb as u64;
        Self([
            narrow(c[0]),
            narrow(c[1]),
            narrow(c[2]),
            narrow(c[3]),
            narrow(c[4]),
        ])
    }

    /// `self^(p-2)`: the inverse (zero for zero).
    fn invert(self) -> Self {
        // p - 2 = 2^255 - 21; square-and-multiply over its bits (the exponent is public).
        let mut result = ONE;
        for bit in (0..255).rev() {
            result = result.square();
            // 2^255 - 21 = all ones in bits 254..5, then 0b01011.
            let one = bit >= 5 || matches!(bit, 3 | 1 | 0);
            if one {
                result = result.mul(self);
            }
        }
        result
    }
}

/// Swaps `a` and `b` when `swap` is 1, without branching on it.
fn cswap(swap: u64, a: &mut Fe, b: &mut Fe) {
    let mask = 0u64.wrapping_sub(swap);
    for i in 0..5 {
        let t = mask & (a.0[i] ^ b.0[i]);
        a.0[i] ^= t;
        b.0[i] ^= t;
    }
}

/// The u-coordinate of the base point.
pub const BASEPOINT: [u8; 32] = {
    let mut b = [0u8; 32];
    b[0] = 9;
    b
};

/// `scalar` times the point with u-coordinate `u` (RFC 7748 section 5): the scalar is clamped first.
// The names are the ones RFC 7748 uses for the ladder's intermediate values.
#[allow(clippy::many_single_char_names)]
#[must_use]
pub fn x25519(scalar: &[u8; 32], u: &[u8; 32]) -> [u8; 32] {
    let mut k = *scalar;
    k[0] &= 0xF8;
    k[31] &= 0x7F;
    k[31] |= 0x40;

    let x1 = Fe::from_bytes(u);
    let (mut x2, mut z2) = (ONE, ZERO);
    let (mut x3, mut z3) = (x1, ONE);
    let mut swap = 0u64;
    for t in (0..255).rev() {
        let bit = u64::from((k[t / 8] >> (t % 8)) & 1);
        swap ^= bit;
        cswap(swap, &mut x2, &mut x3);
        cswap(swap, &mut z2, &mut z3);
        swap = bit;

        let a = x2.add(z2);
        let aa = a.square();
        let b = x2.sub(z2);
        let bb = b.square();
        let e = aa.sub(bb);
        let c = x3.add(z3);
        let d = x3.sub(z3);
        let da = d.mul(a);
        let cb = c.mul(b);
        x3 = da.add(cb).square();
        z3 = x1.mul(da.sub(cb).square());
        x2 = aa.mul(bb);
        z2 = e.mul(aa.add(e.mul_small(121_665)));
    }
    cswap(swap, &mut x2, &mut x3);
    cswap(swap, &mut z2, &mut z3);
    x2.mul(z2.invert()).to_bytes()
}

/// The public key (u-coordinate) for a private scalar.
#[must_use]
pub fn public_key(scalar: &[u8; 32]) -> [u8; 32] {
    x25519(scalar, &BASEPOINT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap();
        }
        out
    }

    #[test]
    fn rfc_7748_section_5_2_vectors() {
        assert_eq!(
            x25519(
                &hex("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4"),
                &hex("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c"),
            ),
            hex("c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552")
        );
        assert_eq!(
            x25519(
                &hex("4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d"),
                &hex("e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493"),
            ),
            hex("95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957")
        );
    }

    #[test]
    fn rfc_7748_iterated_vectors() {
        let mut k = BASEPOINT;
        let mut u = BASEPOINT;
        for round in 1..=1000 {
            let next = x25519(&k, &u);
            u = k;
            k = next;
            if round == 1 {
                assert_eq!(
                    k,
                    hex("422c8e7a6227d7bca1350b3e2bb7279f7897b87bb6854b783c60e80311ae3079")
                );
            }
        }
        assert_eq!(
            k,
            hex("684cf59ba83309552800ef566f2f4d3c1c3887c49360e3875f2eb94d99532c51")
        );
    }

    #[test]
    fn rfc_7748_section_6_1_key_agreement() {
        let alice = hex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let bob = hex("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        let alice_public = public_key(&alice);
        let bob_public = public_key(&bob);
        assert_eq!(
            alice_public,
            hex("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")
        );
        assert_eq!(
            bob_public,
            hex("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f")
        );
        let shared = hex("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
        assert_eq!(x25519(&alice, &bob_public), shared);
        assert_eq!(x25519(&bob, &alice_public), shared);
    }

    #[test]
    fn the_top_bit_of_the_point_is_ignored_and_small_order_points_give_zero() {
        let k = hex("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let mut u = hex("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
        let plain = x25519(&k, &u);
        u[31] |= 0x80;
        assert_eq!(x25519(&k, &u), plain);
        // The point of order 1 (u = 0) and of order 2 (u = 1) have no usable shared secret.
        assert_eq!(x25519(&k, &[0u8; 32]), [0u8; 32]);
    }

    #[test]
    fn field_arithmetic_round_trips() {
        for text in [
            "0000000000000000000000000000000000000000000000000000000000000000",
            "0100000000000000000000000000000000000000000000000000000000000000",
            "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
            "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456709",
        ] {
            let bytes = hex(text);
            let fe = Fe::from_bytes(&bytes);
            let mut expected = bytes;
            expected[31] &= 0x7f;
            // Values at or above p are reduced.
            let back = fe.to_bytes();
            if text.starts_with("ed") {
                assert_eq!(back, [0u8; 32], "p itself is zero");
            } else {
                assert_eq!(back, expected, "{text}");
            }
            // x * x^-1 = 1 for non-zero x.
            if back != [0u8; 32] {
                assert_eq!(fe.mul(fe.invert()).to_bytes(), ONE.to_bytes());
            }
            assert_eq!(fe.sub(fe).to_bytes(), [0u8; 32]);
            assert_eq!(fe.add(ZERO).to_bytes(), back);
        }
    }
}
