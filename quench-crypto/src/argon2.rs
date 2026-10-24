//! Argon2id (RFC 9106) password hashing.
//!
//! Hashes are stored in the PHC string format,
//! `$argon2id$v=19$m=19456,t=2,p=1$<salt>$<hash>`, which carries the
//! parameters, so a hash made with other settings, or by another
//! implementation, still verifies.
//!
//! Only version 0x13 and the `id` variant are supported. Lanes are computed
//! one after another (the result does not depend on that).

// The names are the RFC's: `a`..`d` for the mixing function, `x`/`y` for its
// inputs, `m`, `t`, `p` for the cost parameters.
#![allow(clippy::many_single_char_names)]

use std::fmt;

use quench_core::base64::STANDARD_NO_PAD;

use crate::blake2b::blake2b;
use crate::ct;

const BLOCK_WORDS: usize = 128;
const SYNC_POINTS: usize = 4;
const VERSION: u32 = 0x13;
const TYPE_ID: u32 = 2;

type Block = [u64; BLOCK_WORDS];

/// Cost and output settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    /// Memory in KiB.
    pub memory_kib: u32,
    /// Passes over the memory.
    pub iterations: u32,
    /// Lanes (degree of parallelism).
    pub lanes: u32,
    /// Hash length in bytes.
    pub output_len: usize,
}

impl Default for Params {
    /// 19 MiB, 2 passes, 1 lane, 32 bytes: the OWASP-recommended minimum for
    /// Argon2id, and what the `argon2` crate defaulted to.
    fn default() -> Self {
        Self {
            memory_kib: 19 * 1024,
            iterations: 2,
            lanes: 1,
            output_len: 32,
        }
    }
}

/// Parameters or input the algorithm cannot work with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamsError(&'static str);

impl fmt::Display for ParamsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Argon2 parameters: {}", self.0)
    }
}

impl std::error::Error for ParamsError {}

impl Params {
    fn check(&self, salt: &[u8]) -> Result<(), ParamsError> {
        if self.lanes == 0 || self.lanes > 0x00ff_ffff {
            return Err(ParamsError("lanes must be 1 to 16777215"));
        }
        if self.iterations == 0 {
            return Err(ParamsError("iterations must be at least 1"));
        }
        if self.memory_kib < 8 * self.lanes {
            return Err(ParamsError("memory must be at least 8 KiB per lane"));
        }
        if self.output_len < 4 || u32::try_from(self.output_len).is_err() {
            return Err(ParamsError("the output must be at least 4 bytes"));
        }
        if salt.len() < 8 {
            return Err(ParamsError("the salt must be at least 8 bytes"));
        }
        Ok(())
    }
}

/// The extended hash `H'`: `BLAKE2b` stretched to any output length.
fn long_hash(out_len: usize, input: &[u8]) -> Vec<u8> {
    let mut data = u32::try_from(out_len)
        .unwrap_or(u32::MAX)
        .to_le_bytes()
        .to_vec();
    data.extend_from_slice(input);
    if out_len <= 64 {
        return blake2b(out_len, &data);
    }
    let blocks = out_len.div_ceil(32) - 2;
    let mut out = Vec::with_capacity(out_len);
    let mut v = blake2b(64, &data);
    out.extend_from_slice(&v[..32]);
    for _ in 1..blocks {
        v = blake2b(64, &v);
        out.extend_from_slice(&v[..32]);
    }
    v = blake2b(out_len - 32 * blocks, &v);
    out.extend_from_slice(&v);
    out
}

fn load(block: &mut Block, bytes: &[u8]) {
    let (chunks, _) = bytes.as_chunks::<8>();
    for (word, chunk) in block.iter_mut().zip(chunks) {
        *word = u64::from_le_bytes(*chunk);
    }
}

/// The `BlaMka` round on one row/column of sixteen words.
fn permute(v: &mut [u64; 16]) {
    fn g(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize) {
        let mul = |x: u64, y: u64| {
            2u64.wrapping_mul(x & 0xffff_ffff)
                .wrapping_mul(y & 0xffff_ffff)
        };
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(mul(v[a], v[b]));
        v[d] = (v[d] ^ v[a]).rotate_right(32);
        v[c] = v[c].wrapping_add(v[d]).wrapping_add(mul(v[c], v[d]));
        v[b] = (v[b] ^ v[c]).rotate_right(24);
        v[a] = v[a].wrapping_add(v[b]).wrapping_add(mul(v[a], v[b]));
        v[d] = (v[d] ^ v[a]).rotate_right(16);
        v[c] = v[c].wrapping_add(v[d]).wrapping_add(mul(v[c], v[d]));
        v[b] = (v[b] ^ v[c]).rotate_right(63);
    }
    g(v, 0, 4, 8, 12);
    g(v, 1, 5, 9, 13);
    g(v, 2, 6, 10, 14);
    g(v, 3, 7, 11, 15);
    g(v, 0, 5, 10, 15);
    g(v, 1, 6, 11, 12);
    g(v, 2, 7, 8, 13);
    g(v, 3, 4, 9, 14);
}

/// The compression function: `P(x ^ y) ^ x ^ y`.
fn compress(x: &Block, y: &Block) -> Block {
    let mut r = [0u64; BLOCK_WORDS];
    for i in 0..BLOCK_WORDS {
        r[i] = x[i] ^ y[i];
    }
    let mut q = r;
    for row in 0..8 {
        let mut v = [0u64; 16];
        v.copy_from_slice(&q[row * 16..row * 16 + 16]);
        permute(&mut v);
        q[row * 16..row * 16 + 16].copy_from_slice(&v);
    }
    for col in 0..8 {
        let mut v = [0u64; 16];
        for row in 0..8 {
            v[2 * row] = q[row * 16 + 2 * col];
            v[2 * row + 1] = q[row * 16 + 2 * col + 1];
        }
        permute(&mut v);
        for row in 0..8 {
            q[row * 16 + 2 * col] = v[2 * row];
            q[row * 16 + 2 * col + 1] = v[2 * row + 1];
        }
    }
    for i in 0..BLOCK_WORDS {
        q[i] ^= r[i];
    }
    q
}

/// The raw Argon2id hash of `password`.
pub fn hash_raw(password: &[u8], salt: &[u8], params: &Params) -> Result<Vec<u8>, ParamsError> {
    hash_raw_keyed(password, salt, &[], &[], params)
}

/// Like [`hash_raw`] with the optional secret ("pepper") and associated data
/// that RFC 9106 defines.
pub fn hash_raw_keyed(
    password: &[u8],
    salt: &[u8],
    secret: &[u8],
    associated_data: &[u8],
    params: &Params,
) -> Result<Vec<u8>, ParamsError> {
    params.check(salt)?;
    let lanes = params.lanes as usize;
    let memory_blocks = 4 * lanes * (params.memory_kib as usize / (4 * lanes));
    let lane_len = memory_blocks / lanes;
    let segment_len = lane_len / SYNC_POINTS;

    // H0
    let mut seed = Vec::new();
    for value in [
        params.lanes,
        u32::try_from(params.output_len).unwrap_or(u32::MAX),
        params.memory_kib,
        params.iterations,
        VERSION,
        TYPE_ID,
    ] {
        seed.extend_from_slice(&value.to_le_bytes());
    }
    for input in [password, salt, secret, associated_data] {
        seed.extend_from_slice(&u32::try_from(input.len()).unwrap_or(u32::MAX).to_le_bytes());
        seed.extend_from_slice(input);
    }
    let h0 = blake2b(64, &seed);

    let mut memory = vec![[0u64; BLOCK_WORDS]; memory_blocks];
    for lane in 0..lanes {
        for column in 0..2u32 {
            let mut input = h0.clone();
            input.extend_from_slice(&column.to_le_bytes());
            input.extend_from_slice(&u32::try_from(lane).unwrap_or(0).to_le_bytes());
            load(
                &mut memory[lane * lane_len + column as usize],
                &long_hash(1024, &input),
            );
        }
    }

    for pass in 0..params.iterations as usize {
        for slice in 0..SYNC_POINTS {
            for lane in 0..lanes {
                fill_segment(
                    &mut memory,
                    params,
                    pass,
                    slice,
                    lane,
                    lane_len,
                    segment_len,
                    memory_blocks,
                );
            }
        }
    }

    let mut last = memory[lane_len - 1];
    for lane in 1..lanes {
        for (word, other) in last.iter_mut().zip(&memory[lane * lane_len + lane_len - 1]) {
            *word ^= other;
        }
    }
    let bytes: Vec<u8> = last.iter().flat_map(|w| w.to_le_bytes()).collect();
    Ok(long_hash(params.output_len, &bytes))
}

#[allow(clippy::too_many_arguments)]
fn fill_segment(
    memory: &mut [Block],
    params: &Params,
    pass: usize,
    slice: usize,
    lane: usize,
    lane_len: usize,
    segment_len: usize,
    memory_blocks: usize,
) {
    let lanes = params.lanes as usize;
    // Argon2id: the first half of the first pass picks references from
    // position alone (data-independent), the rest from the data.
    let independent = pass == 0 && slice < SYNC_POINTS / 2;

    let mut address = [0u64; BLOCK_WORDS];
    let mut input = [0u64; BLOCK_WORDS];
    if independent {
        input[0] = pass as u64;
        input[1] = lane as u64;
        input[2] = slice as u64;
        input[3] = memory_blocks as u64;
        input[4] = u64::from(params.iterations);
        input[5] = u64::from(TYPE_ID);
    }
    let next_addresses = |input: &mut Block, address: &mut Block| {
        input[6] += 1;
        let zero = [0u64; BLOCK_WORDS];
        *address = compress(&zero, &compress(&zero, input));
    };

    let start = if pass == 0 && slice == 0 {
        if independent {
            next_addresses(&mut input, &mut address);
        }
        2
    } else {
        0
    };

    for index in start..segment_len {
        let column = slice * segment_len + index;
        let current = lane * lane_len + column;
        let previous = if column == 0 {
            lane * lane_len + lane_len - 1
        } else {
            current - 1
        };

        let random = if independent {
            if index % BLOCK_WORDS == 0 {
                next_addresses(&mut input, &mut address);
            }
            address[index % BLOCK_WORDS]
        } else {
            memory[previous][0]
        };
        let j1 = random & 0xffff_ffff;
        let j2 = random >> 32;

        let ref_lane = if pass == 0 && slice == 0 {
            lane
        } else {
            usize::try_from(j2).unwrap_or(0) % lanes
        };
        let same_lane = ref_lane == lane;
        let area = if pass == 0 {
            if slice == 0 {
                index - 1
            } else if same_lane {
                slice * segment_len + index - 1
            } else {
                slice * segment_len - usize::from(index == 0)
            }
        } else if same_lane {
            lane_len - segment_len + index - 1
        } else {
            lane_len - segment_len - usize::from(index == 0)
        };
        let relative = (j1 * j1) >> 32;
        let relative = area as u64 - 1 - ((area as u64 * relative) >> 32);
        let start_position = if pass == 0 || slice == SYNC_POINTS - 1 {
            0
        } else {
            (slice + 1) * segment_len
        };
        let ref_column = (start_position + usize::try_from(relative).unwrap_or(0)) % lane_len;

        let mut block = compress(&memory[previous], &memory[ref_lane * lane_len + ref_column]);
        if pass > 0 {
            for (word, old) in block.iter_mut().zip(&memory[current]) {
                *word ^= old;
            }
        }
        memory[current] = block;
    }
}

/// The PHC text could not be used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhcError(&'static str);

impl fmt::Display for PhcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid Argon2 hash string: {}", self.0)
    }
}

impl std::error::Error for PhcError {}

fn phc(params: &Params, salt: &[u8], hash: &[u8]) -> String {
    format!(
        "$argon2id$v=19$m={},t={},p={}${}${}",
        params.memory_kib,
        params.iterations,
        params.lanes,
        STANDARD_NO_PAD.encode(salt),
        STANDARD_NO_PAD.encode(hash)
    )
}

/// Hashes a password with a fresh random 16-byte salt.
pub fn hash_password(password: &str, params: &Params) -> Result<String, ParamsError> {
    hash_password_with_salt(password, &quench_core::random::bytes::<16>(), params)
}

/// Hashes a password with the given salt (tests, migrations).
pub fn hash_password_with_salt(
    password: &str,
    salt: &[u8],
    params: &Params,
) -> Result<String, ParamsError> {
    let hash = hash_raw(password.as_bytes(), salt, params)?;
    Ok(phc(params, salt, &hash))
}

/// Parses `$argon2id$v=19$m=..,t=..,p=..$salt$hash`.
fn parse_phc(text: &str) -> Result<(Params, Vec<u8>, Vec<u8>), PhcError> {
    let mut parts = text.split('$');
    if parts.next() != Some("") {
        return Err(PhcError("it must start with `$`"));
    }
    if parts.next() != Some("argon2id") {
        return Err(PhcError("only argon2id is supported"));
    }
    if parts.next() != Some("v=19") {
        return Err(PhcError("only version 19 (0x13) is supported"));
    }
    let settings = parts.next().ok_or(PhcError("the parameters are missing"))?;
    let (Some(salt), Some(hash), None) = (parts.next(), parts.next(), parts.next()) else {
        return Err(PhcError("expected a salt and a hash"));
    };

    let (mut memory, mut iterations, mut lanes) = (None, None, None);
    for setting in settings.split(',') {
        let (name, value) = setting
            .split_once('=')
            .ok_or(PhcError("a parameter is not name=value"))?;
        let number: u32 = value
            .parse()
            .map_err(|_| PhcError("a parameter is not a number"))?;
        let slot = match name {
            "m" => &mut memory,
            "t" => &mut iterations,
            "p" => &mut lanes,
            _ => return Err(PhcError("unknown parameter")),
        };
        if slot.replace(number).is_some() {
            return Err(PhcError("a parameter is repeated"));
        }
    }
    let salt = STANDARD_NO_PAD
        .decode(salt)
        .map_err(|_| PhcError("the salt is not base64"))?;
    let hash = STANDARD_NO_PAD
        .decode(hash)
        .map_err(|_| PhcError("the hash is not base64"))?;
    let params = Params {
        memory_kib: memory.ok_or(PhcError("m is missing"))?,
        iterations: iterations.ok_or(PhcError("t is missing"))?,
        lanes: lanes.ok_or(PhcError("p is missing"))?,
        output_len: hash.len(),
    };
    Ok((params, salt, hash))
}

/// Whether `password` produces the stored `phc` hash. `Ok(false)` means a
/// wrong password; `Err` means the stored text is not a hash this can read.
pub fn verify_password(password: &str, phc: &str) -> Result<bool, PhcError> {
    let (params, salt, expected) = parse_phc(phc)?;
    let actual = hash_raw(password.as_bytes(), &salt, &params)
        .map_err(|_| PhcError("the parameters are out of range"))?;
    Ok(ct::eq(&actual, &expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::hex;

    // RFC 9106 section 5.3: Argon2id, 4 lanes, with a secret and associated data.
    #[quench_test::test]
    fn rfc9106_argon2id_vector() {
        let params = Params {
            memory_kib: 32,
            iterations: 3,
            lanes: 4,
            output_len: 32,
        };
        let tag = hash_raw_keyed(&[1u8; 32], &[2u8; 16], &[3u8; 8], &[4u8; 12], &params).unwrap();
        assert_eq!(
            hex::encode(tag),
            "0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659"
        );
    }

    // Hashes of "password"/"somesalt" produced by the `argon2` crate (argon2id, v0x13).
    #[quench_test::test]
    fn matches_the_argon2_crate() {
        for (memory_kib, iterations, lanes, expected) in [
            (
                64,
                2,
                1,
                "16a1a498734609dd01456da406de9f3d9da93e6c86c300a12fc1465214ce4922",
            ),
            (
                256,
                2,
                1,
                "9dfeb910e80bad0311fee20f9c0e2b12c17987b4cac90c2ef54d5b3021c68bfe",
            ),
            (
                64,
                2,
                2,
                "94387415dfb84ed1977465a1e8626073adf42bd4eeae1faa1dd4e23a1ff6859f",
            ),
            (
                32,
                3,
                4,
                "bb0cc80a3e671149526915418c6eefe761bb19d5d2d567a017703e0cea6ab05c",
            ),
        ] {
            let params = Params {
                memory_kib,
                iterations,
                lanes,
                output_len: 32,
            };
            assert_eq!(
                hex::encode(hash_raw(b"password", b"somesalt", &params).unwrap()),
                expected,
                "m={memory_kib} t={iterations} p={lanes}"
            );
        }
    }

    // What `Argon2::default()` stored for existing users: these must keep verifying.
    #[quench_test::test]
    fn stored_hashes_from_the_argon2_crate_verify() {
        let stored = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$k9ohroejZy1PN5E1SbHNPug6vB8eQAahb5SOAp6iGF4";
        assert!(verify_password("correct horse", stored).unwrap());
        assert!(!verify_password("correct horsf", stored).unwrap());
        // and writing the same password with the same salt gives the same text
        assert_eq!(
            hash_password_with_salt("correct horse", b"somesaltsomesalt", &Params::default())
                .unwrap(),
            stored
        );
    }

    #[quench_test::test]
    fn password_hashes_verify() {
        let params = Params {
            memory_kib: 64,
            iterations: 2,
            lanes: 1,
            output_len: 32,
        };
        let hash = hash_password("hunter2", &params).unwrap();
        assert!(hash.starts_with("$argon2id$v=19$m=64,t=2,p=1$"), "{hash}");
        assert!(verify_password("hunter2", &hash).unwrap());
        assert!(!verify_password("hunter3", &hash).unwrap());
        assert!(!verify_password("", &hash).unwrap());
        // a fresh salt each time
        assert_ne!(hash, hash_password("hunter2", &params).unwrap());
    }

    #[quench_test::test]
    fn defaults_match_the_argon2_crate() {
        let params = Params::default();
        assert_eq!(
            (
                params.memory_kib,
                params.iterations,
                params.lanes,
                params.output_len
            ),
            (19_456, 2, 1, 32)
        );
        let hash = hash_password("pw", &params).unwrap();
        assert!(
            hash.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
            "{hash}"
        );
        assert!(verify_password("pw", &hash).unwrap());
    }

    #[quench_test::test]
    fn long_outputs_use_the_extended_hash() {
        let params = Params {
            memory_kib: 32,
            iterations: 1,
            lanes: 1,
            output_len: 100,
        };
        let raw = hash_raw(b"p", b"saltsalt", &params).unwrap();
        assert_eq!(raw.len(), 100);
        assert_ne!(&raw[..32], &raw[32..64]);
        for len in [4, 16, 33, 64, 65, 96, 97, 130] {
            let p = Params {
                output_len: len,
                ..params
            };
            assert_eq!(hash_raw(b"p", b"saltsalt", &p).unwrap().len(), len);
        }
    }

    #[quench_test::test]
    fn every_input_changes_the_hash() {
        let base = Params {
            memory_kib: 32,
            iterations: 2,
            lanes: 1,
            output_len: 32,
        };
        let reference = hash_raw(b"pw", b"saltsalt", &base).unwrap();
        assert_ne!(reference, hash_raw(b"pW", b"saltsalt", &base).unwrap());
        assert_ne!(reference, hash_raw(b"pw", b"saltsalT", &base).unwrap());
        assert_ne!(
            reference,
            hash_raw(
                b"pw",
                b"saltsalt",
                &Params {
                    memory_kib: 40,
                    ..base
                }
            )
            .unwrap()
        );
        assert_ne!(
            reference,
            hash_raw(
                b"pw",
                b"saltsalt",
                &Params {
                    iterations: 3,
                    ..base
                }
            )
            .unwrap()
        );
        assert_ne!(
            reference,
            hash_raw(
                b"pw",
                b"saltsalt",
                &Params {
                    output_len: 33,
                    ..base
                }
            )
            .unwrap()
        );
        assert_eq!(reference, hash_raw(b"pw", b"saltsalt", &base).unwrap());
    }

    #[quench_test::test]
    fn several_lanes_and_passes() {
        for lanes in [2, 3, 4] {
            for iterations in [1, 3] {
                let params = Params {
                    memory_kib: 8 * lanes * 2,
                    iterations,
                    lanes,
                    output_len: 32,
                };
                let a = hash_raw(b"pw", b"saltsalt", &params).unwrap();
                assert_eq!(a, hash_raw(b"pw", b"saltsalt", &params).unwrap());
                assert_eq!(a.len(), 32);
            }
        }
    }

    #[quench_test::test]
    fn invalid_parameters_are_refused() {
        let ok = Params {
            memory_kib: 32,
            iterations: 1,
            lanes: 1,
            output_len: 32,
        };
        for (params, salt, needle) in [
            (Params { lanes: 0, ..ok }, &b"saltsalt"[..], "lanes"),
            (
                Params {
                    lanes: 0x0100_0000,
                    ..ok
                },
                b"saltsalt",
                "lanes",
            ),
            (
                Params {
                    iterations: 0,
                    ..ok
                },
                b"saltsalt",
                "iterations",
            ),
            (
                Params {
                    memory_kib: 7,
                    ..ok
                },
                b"saltsalt",
                "memory",
            ),
            (
                Params {
                    memory_kib: 15,
                    lanes: 2,
                    ..ok
                },
                b"saltsalt",
                "memory",
            ),
            (
                Params {
                    output_len: 3,
                    ..ok
                },
                b"saltsalt",
                "output",
            ),
            (ok, b"short", "salt"),
        ] {
            let e = hash_raw(b"pw", salt, &params).unwrap_err();
            assert!(e.to_string().contains(needle), "{e}");
        }
    }

    #[quench_test::test]
    fn malformed_hash_strings_are_errors_not_mismatches() {
        for bad in [
            "",
            "plain text",
            "argon2id$v=19$m=64,t=2,p=1$c2FsdHNhbHQ$aGFzaA",
            "$argon2i$v=19$m=64,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=16$m=64,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2,p=1,m=64$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2,p=1,x=1$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2,p=one$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2,p$c2FsdHNhbHQ$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2,p=1$!!!$aGFzaGhhc2g",
            "$argon2id$v=19$m=64,t=2,p=1$c2FsdHNhbHQ$!!!",
            "$argon2id$v=19$m=64,t=2,p=1$c2FsdHNhbHQ",
            "$argon2id$v=19$m=64,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2g$extra",
            "$argon2id$v=19",
            // parameters outside what the algorithm accepts
            "$argon2id$v=19$m=1,t=2,p=1$c2FsdHNhbHQ$aGFzaGhhc2g",
        ] {
            assert!(verify_password("pw", bad).is_err(), "{bad:?}");
        }
        assert!(
            verify_password("pw", "nope")
                .unwrap_err()
                .to_string()
                .contains("invalid Argon2 hash")
        );
    }
}
