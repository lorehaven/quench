//! zstd: the decoder against streams written by the reference encoder, the encoder through the decoder,
//! and hand-built frames for the paths real encoders rarely take.

#![allow(clippy::cast_possible_truncation)]

use quench_archive::Error;
use quench_archive::checksum::crc32;
use quench_archive::zstd::{compress, decompress};

const LIMIT: usize = 1 << 28;

fn assert_docs(packed: &[u8]) {
    let out = decompress(packed, LIMIT).unwrap();
    assert_eq!(out.len(), 161_372);
    assert_eq!(crc32(&out), 0x39A8_FA7A);
}

#[test]
fn reads_streams_from_the_reference_encoder() {
    assert_docs(include_bytes!("fixtures/docs-1.zst"));
    assert_docs(include_bytes!("fixtures/docs-19.zst"));
    assert_docs(include_bytes!("fixtures/docs-nocheck.zst"));
    let small = decompress(include_bytes!("fixtures/small.zst"), LIMIT).unwrap();
    assert_eq!((small.len(), crc32(&small)), (5000, 0x02EB_A687));
}

fn corpus() -> Vec<u8> {
    decompress(include_bytes!("fixtures/docs-19.zst"), LIMIT).unwrap()
}

fn noise(n: usize) -> Vec<u8> {
    let mut x = 99u32;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            u8::try_from((x >> 16) & 0xFF).unwrap()
        })
        .collect()
}

#[test]
fn the_encoder_round_trips() {
    let text = corpus();
    let packed = compress(&text);
    assert_eq!(decompress(&packed, LIMIT).unwrap(), text);
    assert!(packed.len() < text.len() / 2, "{}", packed.len());

    let repeated: Vec<u8> = b"0123456789abcdef".repeat(20_000);
    let packed = compress(&repeated);
    assert_eq!(decompress(&packed, LIMIT).unwrap(), repeated);
    assert!(packed.len() < 2000);

    // Many short sequences (more than 127, so the count takes two bytes).
    let many: Vec<u8> = (0..60_000u32)
        .flat_map(|i| format!("k{} ", i % 300).into_bytes())
        .collect();
    assert_eq!(decompress(&compress(&many), LIMIT).unwrap(), many);

    let utf8 = "Zażółć gęślą jaźń — ünï ☃ ".repeat(3000);
    assert_eq!(
        decompress(&compress(utf8.as_bytes()), LIMIT).unwrap(),
        utf8.as_bytes()
    );

    // Frame sizes with a 2-byte and a 4-byte content size field.
    for n in [0, 1, 2, 255, 256, 300, 65_791, 65_792, 70_000] {
        let data = noise(n);
        assert_eq!(decompress(&compress(&data), LIMIT).unwrap(), data, "{n}");
    }
    for n in [2, 1000, 200_000] {
        let data = vec![b'z'; n];
        assert_eq!(decompress(&compress(&data), LIMIT).unwrap(), data);
    }
}

#[test]
fn incompressible_data_costs_little() {
    let data = noise(500_000);
    let packed = compress(&data);
    assert!(packed.len() < data.len() + 100);
    assert_eq!(decompress(&packed, LIMIT).unwrap(), data);
}

#[test]
fn compression_is_deterministic() {
    let text = corpus();
    assert_eq!(compress(&text), compress(&text));
}

#[test]
fn concatenated_and_skippable_frames() {
    let a = compress(b"first ");
    let b = compress(b"second");
    let mut stream = a;
    stream.extend_from_slice(&0x184D_2A55u32.to_le_bytes());
    stream.extend_from_slice(&3u32.to_le_bytes());
    stream.extend_from_slice(b"xyz");
    stream.extend_from_slice(&b);
    assert_eq!(decompress(&stream, 100).unwrap(), b"first second");
}

/// A frame header for `size` bytes of content in one segment, without a checksum.
fn header(size: u8, checksum: bool) -> Vec<u8> {
    let mut frame = 0xFD2F_B528u32.to_le_bytes().to_vec();
    frame.push(0x20 | if checksum { 4 } else { 0 });
    frame.push(size);
    frame
}

fn block(last: bool, kind: u32, size: usize) -> [u8; 3] {
    let word = u32::from(last) | kind << 1 | (size as u32) << 3;
    [word as u8, (word >> 8) as u8, (word >> 16) as u8]
}

#[test]
fn hand_built_frames() {
    // A raw block, then a run-length block.
    let mut frame = header(7, false);
    frame.extend_from_slice(&block(false, 0, 3));
    frame.extend_from_slice(b"abc");
    frame.extend_from_slice(&block(true, 1, 4));
    frame.push(b'z');
    assert_eq!(decompress(&frame, 100).unwrap(), b"abczzzz");

    // The declared content size must match.
    let mut wrong = header(8, false);
    wrong.extend_from_slice(&block(true, 0, 3));
    wrong.extend_from_slice(b"abc");
    assert!(matches!(decompress(&wrong, 100), Err(Error::Corrupt(_))));

    // A wrong checksum.
    let mut checked = header(3, true);
    checked.extend_from_slice(&block(true, 0, 3));
    checked.extend_from_slice(b"abc");
    checked.extend_from_slice(&[0, 0, 0, 0]);
    assert!(matches!(decompress(&checked, 100), Err(Error::Checksum(_))));

    // A reserved block type.
    let mut reserved = header(0, false);
    reserved.extend_from_slice(&block(true, 3, 0));
    assert!(matches!(decompress(&reserved, 100), Err(Error::Corrupt(_))));
}

#[test]
fn limits_and_damage_are_errors() {
    let text = corpus();
    let packed = compress(&text);
    assert!(matches!(decompress(&packed, 1000), Err(Error::Limit(_))));
    let raw = {
        let mut frame = header(3, false);
        frame.extend_from_slice(&block(true, 0, 3));
        frame.extend_from_slice(b"abc");
        frame
    };
    assert!(matches!(decompress(&raw, 2), Err(Error::Limit(_))));
    let rle = {
        let mut frame = header(9, false);
        frame.extend_from_slice(&block(true, 1, 9));
        frame.push(1);
        frame
    };
    assert!(matches!(decompress(&rle, 2), Err(Error::Limit(_))));

    assert!(matches!(decompress(b"", 10), Err(Error::Truncated)));
    assert!(matches!(
        decompress(b"abcdefgh", 10),
        Err(Error::Corrupt(_))
    ));
    assert!(matches!(
        decompress(&packed[..packed.len() / 2], LIMIT),
        Err(Error::Truncated)
    ));
    for cut in [4, 5, 6, 8, 12] {
        assert!(decompress(&packed[..cut], LIMIT).is_err(), "{cut}");
    }

    // Dictionaries are not supported.
    let mut dictionary = packed.clone();
    dictionary[4] |= 1;
    dictionary.insert(6, 9);
    assert!(matches!(
        decompress(&dictionary, LIMIT),
        Err(Error::Unsupported(_))
    ));

    // Reserved header bit.
    let mut bit = packed;
    bit[4] |= 0x08;
    assert!(matches!(decompress(&bit, LIMIT), Err(Error::Corrupt(_))));

    // Flipping bytes anywhere never panics and mostly errors.
    let small = include_bytes!("fixtures/small.zst");
    for i in 0..small.len() {
        let mut damaged = small.to_vec();
        damaged[i] ^= 0x5A;
        let _ = decompress(&damaged, LIMIT);
        let mut damaged = small.to_vec();
        damaged[i] = damaged[i].wrapping_add(1);
        let _ = decompress(&damaged, LIMIT);
    }
    let big = include_bytes!("fixtures/docs-1.zst");
    for i in (0..big.len()).step_by(97) {
        let mut damaged = big.to_vec();
        damaged[i] ^= 0xA5;
        let _ = decompress(&damaged, LIMIT);
    }
    for cut in (0..small.len()).step_by(7) {
        let _ = decompress(&small[..cut], LIMIT);
    }
    // A skippable frame cut short.
    let mut skip = 0x184D_2A50u32.to_le_bytes().to_vec();
    skip.extend_from_slice(&10u32.to_le_bytes());
    assert!(matches!(decompress(&skip, 10), Err(Error::Truncated)));
}
