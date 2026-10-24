//! The encoder against a reference symbol, every version and level, and the invariants of the layout.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_qr::{EcLevel, Error, QrCode};

/// "Hello, World" at level M, as `qrencode -8 -l M` draws it (the same mask, so every module matches).
const HELLO: [&str; 21] = [
    "#######.##.##.#######",
    "#.....#.#..#..#.....#",
    "#.###.#..#.#..#.###.#",
    "#.###.#.####..#.###.#",
    "#.###.#...#...#.###.#",
    "#.....#..##...#.....#",
    "#######.#.#.#.#######",
    "........##...........",
    "#.##.###.##...#..#.##",
    "#.###..#..#.##...##.#",
    "#####.##.##.##.#...##",
    "##.###.##...#...##.#.",
    "....###.##.#..##....#",
    "........#..#.#..#.#.#",
    "#######.#.#...#.#....",
    "#.....#.##..##.#.##..",
    "#.###.#..#...#.#####.",
    "#.###.#.###.##...###.",
    "#.###.#.##.##.##..#..",
    "#.....#..#.#.####...#",
    "#######.#.######..#..",
];

fn bit(code: &QrCode, x: usize, y: usize) -> u32 {
    u32::from(code.is_dark(x, y))
}

/// The 15 format bits from the first copy next to the top-left finder.
fn format_bits(code: &QrCode) -> u32 {
    let mut bits = 0;
    for i in 0..6 {
        bits |= bit(code, 8, i) << i;
    }
    bits |= bit(code, 8, 7) << 6;
    bits |= bit(code, 8, 8) << 7;
    bits |= bit(code, 7, 8) << 8;
    for i in 9..15 {
        bits |= bit(code, 14 - i, 8) << i;
    }
    bits
}

/// The second copy, split between the top-right and bottom-left finders.
fn format_bits_second_copy(code: &QrCode) -> u32 {
    let size = code.size();
    let mut bits = 0;
    for i in 0..8 {
        bits |= bit(code, size - 1 - i, 8) << i;
    }
    for i in 8..15 {
        bits |= bit(code, 8, size - 15 + i) << i;
    }
    bits
}

fn bch(mut value: u32, generator: u32, degree: u32) -> u32 {
    for _ in 0..degree {
        value = (value << 1) ^ ((value >> (degree - 1)) * generator);
    }
    value
}

const fn level_bits(level: EcLevel) -> u32 {
    match level {
        EcLevel::L => 1,
        EcLevel::M => 0,
        EcLevel::Q => 3,
        EcLevel::H => 2,
    }
}

#[quench_test::test]
fn a_small_symbol_matches_the_reference_encoder() {
    let code = QrCode::new("Hello, World").unwrap();
    assert_eq!(
        (code.version(), code.size(), code.level(), code.mask()),
        (1, 21, EcLevel::M, 3)
    );
    for (y, row) in HELLO.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            assert_eq!(code.is_dark(x, y), ch == '#', "module ({x}, {y})");
        }
    }
    let text = code.to_text();
    assert_eq!(text.lines().count(), 21);
    assert!(
        text.lines()
            .next()
            .unwrap()
            .starts_with("##############  ####")
    );
}

#[quench_test::test]
fn the_smallest_version_that_fits_is_chosen_and_the_limits_are_exact() {
    assert_eq!(QrCode::new("").unwrap().version(), 1);
    assert_eq!(QrCode::new(vec![b'a'; 14]).unwrap().version(), 1);
    assert_eq!(QrCode::new(vec![b'a'; 15]).unwrap().version(), 2);
    assert_eq!(
        QrCode::with_level(vec![b'a'; 17], EcLevel::L)
            .unwrap()
            .version(),
        1
    );
    assert_eq!(
        QrCode::with_level(vec![0xffu8; 2953], EcLevel::L)
            .unwrap()
            .version(),
        40
    );
    assert_eq!(
        QrCode::with_level(vec![0u8; 1273], EcLevel::H)
            .unwrap()
            .version(),
        40
    );
    let error = QrCode::with_level(vec![0u8; 1274], EcLevel::H).unwrap_err();
    assert_eq!(
        error,
        Error::TooLong {
            len: 1274,
            max: 1273
        }
    );
    assert!(error.to_string().contains("1274"));
}

#[quench_test::test]
fn every_version_and_level_lays_out_consistently() {
    for level in [EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H] {
        for len in [0usize, 1, 7, 14, 22, 40, 90, 150, 280, 450, 700, 1000, 1273] {
            let data: Vec<u8> = (0..len)
                .map(|i| u8::try_from(i * 7 % 256).unwrap())
                .collect();
            let code = QrCode::with_level(&data, level).unwrap();
            let size = code.size();
            assert_eq!(size, usize::from(code.version()) * 4 + 17);
            // Finder patterns: a dark ring, a light ring, a dark 3x3 centre, at three corners.
            for (cx, cy) in [(3, 3), (size - 4, 3), (3, size - 4)] {
                assert!(
                    code.is_dark(cx, cy)
                        && code.is_dark(cx + 1, cy + 1)
                        && code.is_dark(cx - 3, cy - 3)
                );
                assert!(!code.is_dark(cx + 2, cy) && !code.is_dark(cx, cy + 2));
            }
            // Timing patterns alternate.
            for i in 8..size - 8 {
                assert_eq!(code.is_dark(i, 6), i % 2 == 0);
                assert_eq!(code.is_dark(6, i), i % 2 == 0);
            }
            // The format information is valid, names this level and mask, and is written twice alike.
            let bits = format_bits(&code);
            assert_eq!(bits, format_bits_second_copy(&code));
            let data_bits = (bits ^ 0x5412) >> 10;
            assert_eq!(data_bits, (level_bits(level) << 3) | u32::from(code.mask()));
            assert_eq!(
                bits ^ 0x5412,
                (data_bits << 10) | (bch(data_bits, 0x537, 10) & 0x3ff)
            );
            assert!(code.is_dark(8, size - 8), "the fixed dark module");
            // Version information (7 and up) is valid and written twice alike.
            if code.version() >= 7 {
                let mut first = 0u32;
                let mut second = 0u32;
                for i in 0..18 {
                    first |= bit(&code, size - 11 + i % 3, i / 3) << i;
                    second |= bit(&code, i / 3, size - 11 + i % 3) << i;
                }
                assert_eq!(first, second);
                let version = u32::from(code.version());
                assert_eq!(first, (version << 12) | (bch(version, 0x1f25, 12) & 0xfff));
            }
        }
    }
}

#[quench_test::test]
fn encoding_is_deterministic_and_data_dependent() {
    assert_eq!(QrCode::new("same").unwrap(), QrCode::new("same").unwrap());
    assert_ne!(QrCode::new("one").unwrap(), QrCode::new("two").unwrap());
    assert!(format!("{:?}", QrCode::new("x").unwrap()).contains("version"));
}

#[quench_test::test]
fn modules_outside_the_symbol_are_light() {
    let code = QrCode::new("x").unwrap();
    assert!(
        !code.is_dark(code.size(), 0) && !code.is_dark(0, code.size()) && !code.is_dark(1000, 1000)
    );
}

#[quench_test::test]
fn the_svg_is_a_scaled_document_with_a_quiet_zone() {
    let code = QrCode::new("Hello, World").unwrap();
    let svg = code.to_svg(200);
    // 21 modules + 2 * 4 quiet = 29; 200 / 29 rounds up to 7 pixels per module.
    assert!(svg.starts_with("<?xml version=\"1.0\""));
    assert!(
        svg.contains("width=\"203\" height=\"203\" viewBox=\"0 0 29 29\""),
        "{svg}"
    );
    assert!(svg.contains("<rect width=\"29\" height=\"29\" fill=\"#fff\"/>"));
    assert!(
        svg.contains("M4 4h7v1h-7z"),
        "the first finder row starts after the quiet zone: {svg}"
    );
    assert!(svg.contains("fill=\"#000\"") && svg.trim_end().ends_with("</svg>"));
    // A smaller request still gives at least one pixel per module.
    assert!(QrCode::new("x").unwrap().to_svg(1).contains("width=\"29\""));
}

#[quench_test::test]
fn a_full_payload_fills_every_version_at_every_level() {
    for level in [EcLevel::L, EcLevel::M, EcLevel::Q, EcLevel::H] {
        let mut previous = 0;
        for version in 1..=40u8 {
            let capacity = QrCode::capacity(version, level);
            assert!(capacity > previous, "capacity grows with the version");
            previous = capacity;
            let data: Vec<u8> = (0..capacity)
                .map(|i| u8::try_from(i % 251).unwrap())
                .collect();
            let code = QrCode::with_level(&data, level).unwrap();
            assert_eq!(code.version(), version, "{capacity} bytes at {level:?}");
            assert_eq!(code.size(), usize::from(version) * 4 + 17);
            // Alignment patterns: the last one sits 7 modules from the far corner of the largest versions.
            if version >= 2 {
                let size = code.size();
                assert!(code.is_dark(size - 7, size - 7) && !code.is_dark(size - 8, size - 7));
            }
        }
    }
}

quench_test::main!();
