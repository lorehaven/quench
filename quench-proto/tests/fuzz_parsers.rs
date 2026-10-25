//! Dependency-free mutation fuzzing of every parser that touches attacker-controlled bytes.
//!
//! Not coverage-guided - `cargo fuzz` finds deeper bugs - but it runs in plain `cargo test`, is
//! deterministic for a given seed, and checks *properties*, not just "did not panic":
//!
//! * no panic, whatever the input;
//! * a parser never claims to have consumed more than it was given;
//! * the chunked decoder gives the same answer however the bytes are split across reads
//!   (a split-dependent parser is a smuggling primitive);
//! * anything accepted is free of the bytes that enable injection (CR, LF, NUL in header values).
//!
//! Tune with `QUENCH_FUZZ_ITERS` (default 20000 per target) and `QUENCH_FUZZ_SEED`. A failure prints
//! the seed and the offending input, so it replays exactly.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
// Truncating casts are how a PRNG turns 64 random bits into a byte or an index.
#![allow(
    clippy::wildcard_imports,
    clippy::cast_possible_truncation,
    clippy::missing_const_for_fn
)]

use quench_proto::cookie::{Cookie, cookie_value, parse_cookie_header};
use quench_proto::h1::decode::BodyDecoder;
use quench_proto::h1::parse::{
    Framing, Limits, find_head_end, parse_request_head, parse_response_head, request_framing,
};
use quench_proto::multipart::parse_boundary;
use quench_proto::uri::Uri;
use quench_proto::ws::{Opcode, decode_frame, encode_frame};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn iters() -> u64 {
    env_u64("QUENCH_FUZZ_ITERS", 20_000)
}

fn seed() -> u64 {
    env_u64("QUENCH_FUZZ_SEED", 0x00C0_FFEE_5EED)
}

/// Bytes that are interesting to HTTP parsers; mutations splice these in.
const TOKENS: &[&[u8]] = &[
    b"\r\n",
    b"\n",
    b"\r",
    b"\0",
    b": ",
    b":",
    b" ",
    b"\t",
    b"%",
    b"%00",
    b"%2e%2e",
    b"../",
    b"//",
    b"HTTP/1.1",
    b"HTTP/1.0",
    b"chunked",
    b"Transfer-Encoding",
    b"Content-Length",
    b"0",
    b"-1",
    b"99999999999999999999",
    b"FFFFFFFFFFFFFFFF",
    b";",
    b"=",
    b",",
    b"\"",
    b"\\",
    b"@",
    b"[",
    b"]",
    b"\xff",
    b"\xc0\xaf",
    b"\xe2\x80\xae",
    b"--",
    b"boundary",
];

fn mutate(rng: &mut Rng, seed_input: &[u8]) -> Vec<u8> {
    let mut data = seed_input.to_vec();
    for _ in 0..=rng.below(6) {
        let at = rng.below(data.len() + 1);
        match rng.below(7) {
            0 if !data.is_empty() => {
                let idx = at.min(data.len() - 1);
                data[idx] ^= 1 << rng.below(8);
            }
            1 if !data.is_empty() => {
                data.remove(at.min(data.len() - 1));
            }
            2 => data.insert(at, rng.next() as u8),
            3 => {
                let t = TOKENS[rng.below(TOKENS.len())];
                data.splice(at..at, t.iter().copied());
            }
            4 if !data.is_empty() => {
                let end = (at + rng.below(32)).min(data.len());
                let chunk = data[at.min(end)..end].to_vec();
                data.splice(at..at, chunk);
            }
            5 => data.truncate(at),
            _ => {
                let n = rng.below(512);
                data.splice(at..at, std::iter::repeat_n(b'A', n));
            }
        }
    }
    data
}

/// Runs `check` on `iters()` mutations of the `seeds`; reports seed and input on any panic.
fn fuzz(target: &str, seeds: &[&[u8]], check: impl Fn(&[u8])) {
    let base = seed();
    for i in 0..iters() {
        let mut rng = Rng::new(base.wrapping_add(i.wrapping_mul(0x9E37_79B9_7F4A_7C15)));
        let pick = rng.below(seeds.len());
        let input = mutate(&mut rng, seeds[pick]);
        assert!(
            catch_unwind(AssertUnwindSafe(|| check(&input))).is_ok(),
            "{target}: property violated (QUENCH_FUZZ_SEED={base}, iteration {i})\ninput: {:?}",
            String::from_utf8_lossy(&input)
        );
    }
}

fn clean(bytes: &[u8]) -> bool {
    !bytes.iter().any(|b| matches!(b, b'\r' | b'\n' | 0))
}

// --- request / response heads ---

const REQUEST_SEEDS: &[&[u8]] = &[
    b"GET /a/b?c=d HTTP/1.1\r\nHost: example.com\r\nAccept: */*\r\n\r\n",
    b"POST /upload HTTP/1.1\r\nHost: x\r\nContent-Length: 5\r\nContent-Type: text/plain\r\n\r\n",
    b"POST /c HTTP/1.1\r\nHost: x\r\nTransfer-Encoding: chunked\r\n\r\n",
    b"GET http://example.com/abs HTTP/1.1\r\nHost: example.com\r\nCookie: a=b; c=d\r\n\r\n",
    b"OPTIONS * HTTP/1.1\r\nHost: x\r\n\r\n",
    b"CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\n\r\n",
];

#[quench_test::test]
async fn request_heads_never_panic_and_accepted_ones_are_clean() {
    let limits = Limits::default();
    let accepted = std::sync::atomic::AtomicU64::new(0);
    fuzz("parse_request_head", REQUEST_SEEDS, |input| {
        if let Some(end) = find_head_end(input) {
            assert!(end <= input.len(), "find_head_end past the buffer");
        }
        let Ok(head) = parse_request_head(input, &limits) else {
            return;
        };
        accepted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert!(
            clean(head.uri.as_str().as_bytes()),
            "CR/LF/NUL in accepted target"
        );
        assert!(head.uri.as_str().len() <= limits.max_uri_bytes);
        assert!(head.headers.len() <= limits.max_headers);
        for (name, value) in head.headers.iter() {
            assert!(clean(name.as_str().as_bytes()), "CR/LF/NUL in header name");
            assert!(
                clean(value.as_bytes()),
                "CR/LF/NUL in accepted header value"
            );
        }
        // Framing is decided from the same head: it must never panic either.
        let _ = request_framing(head.version, &head.headers);
    });
    // Canary: if mutation never reaches the accept path the property checks above prove nothing.
    let accepted = accepted.into_inner();
    assert!(
        accepted * 20 >= iters(),
        "only {accepted}/{} mutations were accepted; the fuzzer is not exercising the happy path",
        iters()
    );
}

#[quench_test::test]
async fn framing_is_never_ambiguous() {
    // If a head is accepted, a message with both Content-Length and Transfer-Encoding must not be.
    let limits = Limits::default();
    fuzz("request_framing", REQUEST_SEEDS, |input| {
        let Ok(head) = parse_request_head(input, &limits) else {
            return;
        };
        let has_cl = head.headers.get("content-length").is_some();
        let has_te = head.headers.get("transfer-encoding").is_some();
        if has_cl && has_te {
            assert!(
                request_framing(head.version, &head.headers).is_err(),
                "CL and TE both present yet framing was accepted"
            );
        }
    });
}

const RESPONSE_SEEDS: &[&[u8]] = &[
    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n",
    b"HTTP/1.1 301 Moved Permanently\r\nLocation: /x\r\nSet-Cookie: a=b; Path=/\r\n\r\n",
    b"HTTP/1.0 404 Not Found\r\nConnection: close\r\n\r\n",
    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n",
];

#[quench_test::test]
async fn response_heads_never_panic() {
    let limits = Limits::default();
    fuzz("parse_response_head", RESPONSE_SEEDS, |input| {
        let _ = parse_response_head(input, &limits);
    });
}

// --- chunked body decoding ---

const CHUNK_SEEDS: &[&[u8]] = &[
    b"5\r\nhello\r\n0\r\n\r\n",
    b"3;ext=1\r\nabc\r\n4\r\ndefg\r\n0\r\nTrailer: x\r\n\r\n",
    b"A\r\n0123456789\r\n0\r\n\r\n",
    b"0\r\n\r\n",
];

/// Feeds `input` split at the given points; returns (decoded, consumed, done) or the error.
fn decode_split(input: &[u8], cuts: &[usize]) -> Result<(Vec<u8>, usize, bool), ()> {
    let mut decoder = BodyDecoder::new(Framing::Chunked);
    let mut out = Vec::new();
    let mut offset = 0;
    let mut points: Vec<usize> = cuts.iter().copied().filter(|c| *c <= input.len()).collect();
    points.push(input.len());
    points.sort_unstable();
    for end in points {
        if end <= offset {
            continue;
        }
        let used = decoder
            .feed(&input[offset..end], &mut out)
            .map_err(|_| ())?;
        assert!(
            used <= end - offset,
            "decoder consumed more than it was given"
        );
        offset += used;
        if decoder.is_done() {
            break;
        }
    }
    Ok((out, offset, decoder.is_done()))
}

#[quench_test::test]
async fn chunked_decoding_is_independent_of_how_reads_are_split() {
    fuzz("chunked decoder", CHUNK_SEEDS, |input| {
        let whole = decode_split(input, &[]);
        let bytewise = decode_split(input, &(1..input.len()).collect::<Vec<_>>());
        let mid = decode_split(input, &[input.len() / 2]);
        assert_eq!(whole, bytewise, "whole-buffer vs byte-at-a-time disagree");
        assert_eq!(whole, mid, "whole-buffer vs split-in-half disagree");
        if let Ok((_, consumed, true)) = whole {
            // A finished body must leave pipelined bytes alone.
            assert!(consumed <= input.len());
        }
    });
}

#[quench_test::test]
async fn length_decoder_never_over_reads() {
    let mut rng = Rng::new(seed());
    for _ in 0..iters() {
        let want = rng.below(2048) as u64;
        let have = rng.below(4096);
        let input = vec![b'x'; have];
        let mut decoder = BodyDecoder::new(Framing::Length(want));
        let mut out = Vec::new();
        let used = decoder
            .feed(&input, &mut out)
            .expect("length bodies cannot be malformed");
        assert_eq!(used as u64, want.min(have as u64));
        assert_eq!(out.len(), used);
        assert_eq!(decoder.is_done(), want <= have as u64);
    }
}

// --- WebSocket frames ---

#[quench_test::test]
async fn websocket_frames_round_trip() {
    let mut rng = Rng::new(seed());
    for _ in 0..iters() {
        let len = match rng.below(4) {
            0 => rng.below(126),
            1 => 126 + rng.below(200),
            2 => 65_536 + rng.below(1024),
            _ => rng.below(8),
        };
        let payload: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        let mask = (rng.below(2) == 0).then(|| (rng.next() as u32).to_be_bytes());
        let mut wire = Vec::new();
        encode_frame(&mut wire, true, Opcode::Binary, &payload, mask);
        let (frame, used) = decode_frame(&wire, mask.is_some(), usize::MAX)
            .expect("a frame we encoded must decode")
            .expect("a whole frame must not look partial");
        assert_eq!(used, wire.len());
        assert_eq!(frame.payload, payload);
        // Every strict prefix is "need more", never an error and never a frame.
        let cut = rng.below(wire.len());
        assert!(
            matches!(
                decode_frame(&wire[..cut], mask.is_some(), usize::MAX),
                Ok(None)
            ),
            "prefix of length {cut} of a valid frame was not 'incomplete'"
        );
    }
}

#[quench_test::test]
async fn websocket_decoder_survives_garbage_and_honours_the_payload_cap() {
    let mut valid = Vec::new();
    encode_frame(&mut valid, true, Opcode::Text, b"hello", Some([1, 2, 3, 4]));
    let mut ping = Vec::new();
    encode_frame(&mut ping, true, Opcode::Ping, b"p", Some([9, 9, 9, 9]));
    let mut big = Vec::new();
    encode_frame(
        &mut big,
        true,
        Opcode::Binary,
        &vec![7; 300],
        Some([5, 6, 7, 8]),
    );
    let seeds: Vec<&[u8]> = vec![&valid, &ping, &big];
    fuzz("ws decode_frame", &seeds, |input| {
        for cap in [0usize, 4, 64, 1 << 20] {
            if let Ok(Some((frame, used))) = decode_frame(input, true, cap) {
                assert!(used <= input.len(), "consumed past the buffer");
                assert!(frame.payload.len() <= cap, "payload exceeds the cap {cap}");
            }
        }
        let _ = decode_frame(input, false, 1024);
    });
}

#[quench_test::test]
async fn websocket_declared_huge_lengths_are_refused_before_allocation() {
    // 2^62-byte frame header with a 64 KiB cap: must error immediately, not wait for the payload.
    let mut wire = vec![0x82, 0xff];
    wire.extend_from_slice(&(1u64 << 62).to_be_bytes());
    wire.extend_from_slice(&[0, 0, 0, 0]);
    assert!(decode_frame(&wire, true, 64 * 1024).is_err());
}

// --- URI, cookies, multipart boundary ---

const URI_SEEDS: &[&[u8]] = &[
    b"/a/b/c?x=1&y=2#frag",
    b"http://user:pw@example.com:8080/p?q",
    b"https://[::1]:443/",
    b"//host/path",
    b"*",
    b"example.com:443",
];

#[quench_test::test]
async fn uri_parsing_never_panics_and_components_are_consistent() {
    fuzz("Uri::parse", URI_SEEDS, |input| {
        let Ok(text) = std::str::from_utf8(input) else {
            return;
        };
        let Ok(uri) = Uri::parse(text) else {
            return;
        };
        // Accessors must be callable on anything the parser accepted.
        let _ = (
            uri.scheme(),
            uri.authority(),
            uri.host(),
            uri.port_u16(),
            uri.query(),
        );
        assert!(
            text.contains(uri.path()),
            "path is not a slice of the input"
        );
        let _ = uri.resolve("../../x");
        let _ = uri.resolve("//evil.example/");
    });
}

#[quench_test::test]
async fn cookie_parsing_never_panics() {
    let seeds: &[&[u8]] = &[
        b"session=abc; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=3600",
        b"a=b; c=d; e=\"quoted; value\"",
        b"=novalue; x",
    ];
    fuzz("cookies", seeds, |input| {
        let Ok(text) = std::str::from_utf8(input) else {
            return;
        };
        let _ = Cookie::parse(text);
        let _ = parse_cookie_header(text).count();
        let _ = cookie_value(text, "session");
    });
}

#[quench_test::test]
async fn multipart_boundary_parsing_never_panics() {
    let seeds: &[&[u8]] = &[
        b"multipart/form-data; boundary=----abc123",
        b"multipart/form-data; boundary=\"quoted boundary\"",
        b"multipart/form-data;boundary=x;charset=utf-8",
    ];
    fuzz("parse_boundary", seeds, |input| {
        if let Ok(text) = std::str::from_utf8(input)
            && let Ok(boundary) = parse_boundary(text)
        {
            assert!(
                !boundary.is_empty() && boundary.len() <= 70,
                "boundary out of RFC bounds"
            );
            assert!(clean(boundary.as_bytes()));
        }
    });
}

quench_test::main!();
