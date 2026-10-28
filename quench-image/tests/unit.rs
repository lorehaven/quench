// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::trivially_copy_pass_by_ref,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::many_single_char_names,
    clippy::cast_precision_loss
)]

use quench_archive::{checksum, deflate};
use quench_image::{Error, Image, decode, encode_png};

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend(checksum::crc32(&body).to_be_bytes());
}

/// A PNG around `raw` (the rows with their filter bytes, in the order the file stores them).
fn build(
    width: u32,
    height: u32,
    depth: u8,
    color: u8,
    interlace: u8,
    raw: &[u8],
    extra: &[(&[u8; 4], Vec<u8>)],
) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut header = Vec::new();
    header.extend(width.to_be_bytes());
    header.extend(height.to_be_bytes());
    header.extend([depth, color, 0, 0, interlace]);
    chunk(&mut out, b"IHDR", &header);
    for (kind, data) in extra {
        chunk(&mut out, kind, data);
    }
    let mut zlib = vec![0x78, 0x9c];
    zlib.extend(deflate::compress(raw));
    zlib.extend(checksum::adler32(raw).to_be_bytes());
    // Split the data over two IDAT chunks: a decoder must join them.
    let (a, b) = zlib.split_at(zlib.len() / 2);
    chunk(&mut out, b"IDAT", a);
    chunk(&mut out, b"IDAT", b);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i32::from(a), i32::from(b), i32::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// The forward filter `kind` over `rows` (unfiltered, `row` bytes each, `bpp` bytes a pixel).
fn filter(rows: &[u8], row: usize, bpp: usize, kind: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for (y, line) in rows.chunks(row).enumerate() {
        out.push(kind);
        for i in 0..row {
            let a = if i >= bpp { line[i - bpp] } else { 0 };
            let b = if y > 0 { rows[(y - 1) * row + i] } else { 0 };
            let c = if y > 0 && i >= bpp {
                rows[(y - 1) * row + i - bpp]
            } else {
                0
            };
            let predicted = match kind {
                0 => 0,
                1 => a,
                2 => b,
                3 => u16::midpoint(u16::from(a), u16::from(b)) as u8,
                _ => paeth(a, b, c),
            };
            out.push(line[i].wrapping_sub(predicted));
        }
    }
    out
}

#[quench_test::test]
fn rgba_survives_the_encoder_and_the_decoder() {
    let rgba: Vec<u8> = (0..5 * 3 * 4).map(|i| (i * 7 % 256) as u8).collect();
    let png = encode_png(5, 3, &rgba);
    assert_eq!(
        decode(&png).unwrap(),
        Image {
            width: 5,
            height: 3,
            rgba
        }
    );
}

#[quench_test::test]
fn every_row_filter_is_undone() {
    let pixels: Vec<u8> = (0..4 * 4 * 3)
        .map(|i| ((i * 37 + i / 5) % 256) as u8)
        .collect();
    for kind in 0..=4 {
        let png = build(4, 4, 8, 2, 0, &filter(&pixels, 12, 3, kind), &[]);
        let image = decode(&png).unwrap();
        for (i, px) in image.rgba.chunks(4).enumerate() {
            assert_eq!(
                &px[..3],
                &pixels[i * 3..i * 3 + 3],
                "filter {kind}, pixel {i}"
            );
            assert_eq!(px[3], 255);
        }
    }
}

#[quench_test::test]
fn grey_and_palette_images_of_every_depth() {
    // 1 bit: 8 pixels in a byte, MSB first.
    let png = build(8, 1, 1, 0, 0, &[0, 0b1010_0000], &[]);
    let g = decode(&png).unwrap();
    assert_eq!(
        g.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>(),
        [255, 0, 255, 0, 0, 0, 0, 0]
    );
    // 2 and 4 bits scale to the full range.
    let png = build(4, 1, 2, 0, 0, &[0, 0b00_01_10_11], &[]);
    assert_eq!(
        decode(&png)
            .unwrap()
            .rgba
            .chunks(4)
            .map(|p| p[0])
            .collect::<Vec<_>>(),
        [0, 85, 170, 255]
    );
    let png = build(2, 1, 4, 0, 0, &[0, 0x0f], &[]);
    assert_eq!(
        decode(&png)
            .unwrap()
            .rgba
            .chunks(4)
            .map(|p| p[0])
            .collect::<Vec<_>>(),
        [0, 255]
    );
    // 16 bits keeps the high byte; grey with alpha.
    let png = build(1, 1, 16, 0, 0, &[0, 0x80, 0x01], &[]);
    assert_eq!(&decode(&png).unwrap().rgba[..3], &[0x80, 0x80, 0x80]);
    let png = build(1, 1, 8, 4, 0, &[0, 100, 50], &[]);
    assert_eq!(decode(&png).unwrap().rgba, [100, 100, 100, 50]);
    // A palette with transparency for the first entry.
    let plte = vec![10, 20, 30, 200, 100, 0];
    let png = build(
        2,
        1,
        8,
        3,
        0,
        &[0, 0, 1],
        &[(b"PLTE", plte), (b"tRNS", vec![0])],
    );
    assert_eq!(
        decode(&png).unwrap().rgba,
        [10, 20, 30, 0, 200, 100, 0, 255]
    );
    // A 4-bit palette index.
    let png = build(
        2,
        1,
        4,
        3,
        0,
        &[0, 0x01],
        &[(b"PLTE", vec![1, 2, 3, 4, 5, 6])],
    );
    assert_eq!(decode(&png).unwrap().rgba, [1, 2, 3, 255, 4, 5, 6, 255]);
}

#[quench_test::test]
fn colour_keys_make_pixels_transparent() {
    let png = build(
        2,
        1,
        8,
        2,
        0,
        &[0, 1, 2, 3, 4, 5, 6],
        &[(b"tRNS", vec![0, 1, 0, 2, 0, 3])],
    );
    assert_eq!(decode(&png).unwrap().rgba, [1, 2, 3, 0, 4, 5, 6, 255]);
    let png = build(2, 1, 8, 0, 0, &[0, 7, 9], &[(b"tRNS", vec![0, 7])]);
    assert_eq!(decode(&png).unwrap().rgba, [7, 7, 7, 0, 9, 9, 9, 255]);
    let png = build(1, 1, 16, 6, 0, &[0, 1, 0, 2, 0, 3, 0, 4, 0], &[]);
    assert_eq!(decode(&png).unwrap().rgba, [1, 2, 3, 4]);
}

#[quench_test::test]
fn adam7_interlacing_puts_every_pixel_where_it_belongs() {
    let (w, h) = (9usize, 10usize);
    let grey: Vec<u8> = (0..w * h).map(|i| (i * 2 + 1) as u8).collect();
    let passes = [
        (0, 0, 8, 8),
        (4, 0, 8, 8),
        (0, 4, 4, 8),
        (2, 0, 4, 4),
        (0, 2, 2, 4),
        (1, 0, 2, 2),
        (0, 1, 1, 2),
    ];
    let mut raw = Vec::new();
    for (x0, y0, dx, dy) in passes {
        let (pw, ph) = (
            (w.saturating_sub(x0)).div_ceil(dx),
            (h.saturating_sub(y0)).div_ceil(dy),
        );
        if pw == 0 || ph == 0 {
            continue;
        }
        for y in 0..ph {
            raw.push(0);
            for x in 0..pw {
                raw.push(grey[(y0 + y * dy) * w + x0 + x * dx]);
            }
        }
    }
    let png = build(w as u32, h as u32, 8, 0, 1, &raw, &[]);
    let image = decode(&png).unwrap();
    assert_eq!(image.rgba.chunks(4).map(|p| p[0]).collect::<Vec<_>>(), grey);
    // A tiny interlaced image where some passes are empty.
    let png = build(1, 1, 8, 0, 1, &[0, 77], &[]);
    assert_eq!(decode(&png).unwrap().rgba, [77, 77, 77, 255]);
}

#[quench_test::test]
fn damaged_files_are_errors() {
    let good = encode_png(2, 2, &[255; 16]);
    assert!(decode(&good).is_ok());
    // A flipped byte anywhere is caught by a checksum or a check on the data: never a panic.
    for i in 8..good.len() {
        let mut bad = good.clone();
        bad[i] ^= 0x55;
        let _ = decode(&bad);
    }
    for cut in [0, 7, 8, 20, good.len() - 13, good.len() - 1] {
        assert!(decode(&good[..cut]).is_err(), "cut at {cut}");
    }
    let mut bad_crc = good;
    bad_crc[29] ^= 1;
    assert_eq!(
        decode(&bad_crc),
        Err(Error::Corrupt("a chunk's checksum is wrong"))
    );
    assert!(
        matches!(
            decode(&build(2, 2, 8, 2, 0, &[0; 3], &[])),
            Err(Error::Corrupt(_))
        ),
        "too little data"
    );
    assert!(
        matches!(
            decode(&build(2, 2, 8, 2, 0, &[9; 14], &[])),
            Err(Error::Corrupt(_))
        ),
        "an unknown filter"
    );
    assert!(
        matches!(
            decode(&build(1, 1, 8, 3, 0, &[0, 0], &[])),
            Err(Error::Corrupt(_))
        ),
        "no palette"
    );
    assert!(
        matches!(
            decode(&build(1, 1, 3, 0, 0, &[0, 0], &[])),
            Err(Error::Corrupt(_))
        ),
        "a depth PNG does not have"
    );
    assert!(matches!(
        decode(&build(0, 1, 8, 0, 0, &[], &[])),
        Err(Error::Corrupt(_))
    ));
    assert_eq!(
        decode(&build(20000, 1, 8, 0, 0, &[], &[])),
        Err(Error::TooLarge)
    );
    assert_eq!(
        decode(&build(16000, 16000, 8, 0, 0, &[], &[])),
        Err(Error::TooLarge)
    );
    assert!(
        matches!(
            decode(&build(1, 1, 8, 3, 0, &[0, 0], &[(b"PLTE", vec![1, 2])])),
            Err(Error::Corrupt(_))
        ),
        "a palette of the wrong size"
    );
}

#[quench_test::test]
fn other_formats_are_named() {
    for (bytes, name) in [(&b"<svg xmlns"[..], "SVG"), (b"nothing", "unknown")] {
        let e = decode(bytes).unwrap_err();
        assert_eq!(e, Error::Unsupported(name));
        assert!(e.to_string().contains(name));
    }
    assert!(Error::TooLarge.to_string().contains("large"));
    assert!(Error::Corrupt("x").to_string().contains('x'));
}

/// A picture the fixtures were made from, as another decoder (Pillow, libjpeg) read it.
fn fixture(name: &str) -> (Vec<u8>, Image) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
    let file = std::fs::read(format!("{dir}{name}")).unwrap();
    let rgba = std::fs::read(format!("{dir}{name}.rgba")).unwrap();
    (
        file,
        Image {
            width: 37,
            height: 21,
            rgba,
        },
    )
}

/// How far two pictures are apart: the mean and the largest difference of any channel.
fn distance(a: &[u8], b: &[u8]) -> (f64, u8) {
    assert_eq!(a.len(), b.len());
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    (
        total as f64 / a.len() as f64,
        a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap(),
    )
}

#[quench_test::test]
fn jpegs_match_another_decoder_baseline_progressive_subsampled_and_with_restarts() {
    for name in [
        "base444.jpg",
        "base420.jpg",
        "prog444.jpg",
        "prog420.jpg",
        "restart.jpg",
        "progrestart.jpg",
        "grey.jpg",
    ] {
        let (file, want) = fixture(name);
        let got = decode(&file).unwrap();
        assert_eq!((got.width, got.height), (want.width, want.height), "{name}");
        let (mean, worst) = distance(&got.rgba, &want.rgba);
        // Chroma is smoothed by the other decoder when it is subsampled; here it is repeated.
        let subsampled = name.contains("420") || name.contains("restart");
        assert!(
            mean < if subsampled { 6.0 } else { 1.0 },
            "{name}: mean {mean}"
        );
        assert!(
            worst < if subsampled { 80 } else { 8 },
            "{name}: worst {worst}"
        );
    }
}

#[quench_test::test]
fn gifs_keep_their_palette_transparency_and_interlacing() {
    for name in ["plain.gif", "transparent.gif"] {
        let (file, want) = fixture(name);
        let got = decode(&file).unwrap();
        // A clear pixel's colour does not matter.
        for (g, w) in got.rgba.chunks(4).zip(want.rgba.chunks(4)) {
            assert_eq!(g[3], w[3], "{name}");
            if w[3] != 0 {
                assert_eq!(g, w, "{name}");
            }
        }
    }
    let (file, _) = fixture("transparent.gif");
    assert!(
        decode(&file).unwrap().rgba.chunks(4).any(|p| p[3] == 0),
        "some pixels are clear"
    );
}

#[quench_test::test]
fn damaged_jpegs_and_gifs_are_errors_not_panics() {
    for name in ["prog420.jpg", "base420.jpg", "transparent.gif"] {
        let (file, _) = fixture(name);
        // Cut anywhere, and flip bytes: never a panic.
        for end in (0..file.len()).step_by(7) {
            let _ = decode(&file[..end]);
        }
        for at in (2..file.len()).step_by(5) {
            let mut bad = file.clone();
            bad[at] ^= 0x5a;
            let _ = decode(&bad);
        }
    }
    assert!(decode(&[0xff, 0xd8, 0xff, 0xd9]).is_err());
    assert!(matches!(decode(b"GIF89a"), Err(Error::Corrupt(_))));
    // Four-channel and arithmetic-coded files are named, not guessed.
    let (mut file, _) = fixture("base444.jpg");
    let sof = file.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
    file[sof + 1] = 0xc9;
    assert!(matches!(decode(&file), Err(Error::Unsupported(_))));
}

fn expected(name: &str) -> (Vec<u8>, Vec<u8>) {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/");
    (
        std::fs::read(format!("{dir}{name}")).unwrap(),
        std::fs::read(format!("{dir}{name}.rgba")).unwrap(),
    )
}

#[quench_test::test]
fn lossless_webp_matches_the_pixels_libwebp_makes_of_the_same_files() {
    for name in [
        "lossless.webp",
        "lossless_alpha.webp",
        "lossless_blocks.webp",
        "lossless_palette2.webp",
        "lossless_palette4.webp",
        "lossless_palette16.webp",
        "lossless_palette200.webp",
        "lossless_noise.webp",
    ] {
        let (file, want) = expected(name);
        let got = decode(&file).unwrap();
        assert_eq!(got.rgba.len(), want.len(), "{name}");
        assert!(got.rgba == want, "{name}: pixels differ");
    }
}

#[quench_test::test]
fn the_first_frame_of_an_animated_webp_is_shown() {
    let (file, want) = expected("anim.webp");
    let got = decode(&file).unwrap();
    assert_eq!((got.width, got.height), (30, 20));
    assert!(got.rgba == want, "the first frame, whole");
}

#[quench_test::test]
fn lossy_webp_matches_libwebp_to_the_last_bit_in_every_option_it_makes() {
    for name in [
        "lossy.webp",
        "lossy_alpha.webp",
        "lossy_q10.webp",
        "lossy_big.webp",
        "lossy_simple_filter.webp",
        "lossy_sharp_filter.webp",
        "lossy_partitions.webp",
        "lossy_one_segment.webp",
        "lossy_tiny.webp",
        "lossy_odd.webp",
        "lossy_no_filter.webp",
        "lossy_alpha0.webp",
        "lossy_alpha1.webp",
        "lossy_alpha2.webp",
        "lossy_alpha3.webp",
        "lossy_alpha4.webp",
    ] {
        let (file, want) = expected(name);
        let got = decode(&file).unwrap();
        assert_eq!(got.rgba.len(), want.len(), "{name}");
        let (mean, worst) = distance(&got.rgba, &want);
        assert_eq!(worst, 0, "{name}: mean {mean}");
    }
}

#[quench_test::test]
fn damaged_webp_files_are_errors_and_never_panics() {
    for name in [
        "lossless_alpha.webp",
        "lossless_palette16.webp",
        "lossy_alpha1.webp",
        "lossy_partitions.webp",
        "anim.webp",
    ] {
        let (file, _) = expected(name);
        for end in (0..file.len()).step_by(3) {
            let _ = decode(&file[..end]);
        }
        for at in (12..file.len()).step_by(2) {
            for flip in [0x01u8, 0x80, 0xff] {
                let mut bad = file.clone();
                bad[at] ^= flip;
                let _ = decode(&bad);
            }
        }
    }
    assert!(decode(b"RIFF\x04\0\0\0WEBP").is_err());
    // A frame that is not a key frame, a chunk that runs past the file, a size that is too large, no picture.
    let (mut lossy, _) = expected("lossy.webp");
    let at = lossy.windows(4).position(|w| w == b"VP8 ").unwrap() + 8;
    lossy[at] |= 1;
    assert!(matches!(decode(&lossy), Err(Error::Unsupported(_))));
    let (mut lossless, _) = expected("lossless.webp");
    let at = lossless.windows(4).position(|w| w == b"VP8L").unwrap();
    lossless[at + 4] = 0xff;
    lossless[at + 5] = 0xff;
    assert!(decode(&lossless).is_err());
    lossless[at + 4..at + 8].copy_from_slice(&[0, 0, 0, 0]);
    assert!(decode(&lossless).is_err());
    let mut huge = expected("lossless.webp").0;
    huge[at + 9..at + 13].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
    assert!(decode(&huge).is_err());
    let mut text = b"RIFF\0\0\0\0WEBPJUNK\x04\0\0\0abcd".to_vec();
    let size = (text.len() - 8) as u8;
    text[4] = size;
    assert!(matches!(decode(&text), Err(Error::Corrupt(_))));
}

quench_test::main!();
