//! Unit tests for the `graphics` module: kitty sequences, the sixel encoder (checked by decoding its
//! output again), the reply parsers on real terminals' answers, and the detection loop on canned replies.

use quench_cli::graphics::Rgb;
use quench_cli::graphics::detect::{
    Capabilities, Environment, Forced, Plan, Protocol, QUERIES, plan, probe, resolve,
};
use quench_cli::graphics::kitty::{self, CHUNK, Placement};
use quench_cli::graphics::reply::{
    KittyReply, Replies, WindowReport, parse_device_attributes, parse_kitty_reply,
    parse_osc_colour, parse_replies, parse_window_report,
};
use quench_cli::graphics::sixel::{self, SixelError, as_shown, blend};
use quench_core::base64::STANDARD;
use std::cell::Cell;
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------------
// kitty

#[quench_test::test]
fn kitty_sends_a_small_png_in_one_sequence() {
    assert_eq!(
        kitty::transmit_png(b"abc", 7, Placement::NATURAL),
        "\x1b_Ga=T,f=100,i=7,q=2;YWJj\x1b\\"
    );
    assert_eq!(
        kitty::transmit_png(b"abc", 7, Placement::cells(10, 2)),
        "\x1b_Ga=T,f=100,i=7,q=2,c=10,r=2;YWJj\x1b\\"
    );
    let rows_only = Placement {
        rows: Some(1),
        keep_cursor: true,
        ..Placement::default()
    };
    assert_eq!(
        kitty::transmit_png(b"", 1, rows_only),
        "\x1b_Ga=T,f=100,i=1,q=2,r=1,C=1;\x1b\\"
    );
}

/// The base64 chunks of a chunked transmission, after checking the framing.
fn chunks_of(sequence: &str) -> Vec<(String, String)> {
    let body = sequence.strip_prefix("\x1b_G").unwrap();
    body.strip_suffix("\x1b\\")
        .unwrap()
        .split("\x1b\\\x1b_G")
        .map(|part| {
            let (keys, data) = part.split_once(';').unwrap();
            (keys.to_string(), data.to_string())
        })
        .collect()
}

#[quench_test::test]
fn kitty_chunks_at_4096_bytes_of_base64() {
    // 3072 bytes are exactly 4096 base64 characters: still one sequence.
    let exact = vec![0x5au8; 3072];
    let one = kitty::transmit_png(&exact, 2, Placement::NATURAL);
    assert_eq!(chunks_of(&one).len(), 1);
    assert!(!one.contains("m="));

    // One byte more spills into a second chunk.
    let over = vec![0xa5u8; 3073];
    let parts = chunks_of(&kitty::transmit_png(&over, 2, Placement::cells(3, 1)));
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].0, "a=T,f=100,i=2,q=2,c=3,r=1,m=1");
    assert_eq!(parts[0].1.len(), CHUNK);
    assert_eq!(parts[1].0, "m=0");
    assert_eq!(parts[1].1.len(), 4);

    // Several chunks: only the first carries the keys, the middle ones say more is coming.
    let data: Vec<u8> = (0..10_000u32).map(|n| (n % 251) as u8).collect();
    let parts = chunks_of(&kitty::transmit_png(&data, 9, Placement::NATURAL));
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0].0, "a=T,f=100,i=9,q=2,m=1");
    assert_eq!(parts[1].0, "m=1");
    assert_eq!(parts[2].0, "m=1");
    assert_eq!(parts[3].0, "m=0");
    assert!(parts.iter().all(|(_, chunk)| chunk.len() <= CHUNK));
    let joined: String = parts.iter().map(|(_, chunk)| chunk.as_str()).collect();
    assert_eq!(STANDARD.decode(joined).unwrap(), data);
}

#[quench_test::test]
fn kitty_places_and_deletes_by_id() {
    assert_eq!(
        kitty::place(42, Placement::NATURAL),
        "\x1b_Ga=p,i=42,q=2\x1b\\"
    );
    assert_eq!(
        kitty::place(42, Placement::cells(5, 1)),
        "\x1b_Ga=p,i=42,q=2,c=5,r=1\x1b\\"
    );
    assert_eq!(kitty::delete(42), "\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
    assert_eq!(kitty::QUERY, "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\");
}

// ---------------------------------------------------------------------------------------------------
// sixel, and a small decoder to check it

/// A decoded sixel image: the size from the raster attributes and each pixel's colour, `None` where
/// nothing was drawn.
#[derive(Debug, PartialEq, Eq)]
struct Decoded {
    width: usize,
    height: usize,
    pixels: Vec<Option<Rgb>>,
    palette_size: usize,
}

fn number(bytes: &[u8], at: &mut usize) -> usize {
    let mut n = 0;
    while *at < bytes.len() && bytes[*at].is_ascii_digit() {
        n = n * 10 + usize::from(bytes[*at] - b'0');
        *at += 1;
    }
    n
}

/// Numbers separated by `;`, starting at `at`.
fn numbers(bytes: &[u8], at: &mut usize) -> Vec<usize> {
    let mut list = vec![number(bytes, at)];
    while bytes.get(*at) == Some(&b';') {
        *at += 1;
        list.push(number(bytes, at));
    }
    list
}

fn decode_sixel(text: &str) -> Decoded {
    let bytes = text.as_bytes();
    assert!(text.starts_with("\x1bP0;1;0q"), "header of {text:?}");
    assert!(text.ends_with("\x1b\\"));
    let bytes = &bytes[8..bytes.len() - 2];
    let mut at = 0;
    assert_eq!(bytes[at], b'"');
    at += 1;
    let raster = numbers(bytes, &mut at);
    assert_eq!(&raster[..2], &[1, 1]);
    let (width, height) = (raster[2], raster[3]);
    let mut pixels = vec![None; width * height];
    let mut palette: Vec<Option<Rgb>> = vec![None; 256];
    let mut defined = 0;
    let (mut x, mut band, mut colour) = (0usize, 0usize, 0usize);
    let to_8bit = |p: usize| u8::try_from((p * 255 + 50) / 100).unwrap();
    let mut draw = |x: usize, band: usize, sixel: u8, colour: Option<Rgb>| {
        for bit in 0..6 {
            if sixel & (1 << bit) != 0 {
                let y = band * 6 + bit;
                assert!(x < width && y < height, "pixel {x},{y} outside the raster");
                pixels[y * width + x] = Some(colour.expect("drawing with an undefined colour"));
            }
        }
    };
    while at < bytes.len() {
        match bytes[at] {
            b'#' => {
                at += 1;
                let args = numbers(bytes, &mut at);
                if args.len() == 1 {
                    colour = args[0];
                } else {
                    assert_eq!(args.len(), 5);
                    assert_eq!(args[1], 2, "RGB colour space");
                    assert!(args[2..].iter().all(|&p| p <= 100));
                    palette[args[0]] = Some(Rgb::new(
                        to_8bit(args[2]),
                        to_8bit(args[3]),
                        to_8bit(args[4]),
                    ));
                    defined += 1;
                }
            }
            b'!' => {
                at += 1;
                let count = number(bytes, &mut at);
                let sixel = bytes[at] - 0x3f;
                at += 1;
                for _ in 0..count {
                    draw(x, band, sixel, palette[colour]);
                    x += 1;
                }
            }
            b'$' => {
                x = 0;
                at += 1;
            }
            b'-' => {
                x = 0;
                band += 1;
                at += 1;
            }
            c @ 0x3f..=0x7e => {
                draw(x, band, c - 0x3f, palette[colour]);
                x += 1;
                at += 1;
            }
            other => panic!("unexpected byte {other:#x} in sixel data"),
        }
    }
    Decoded {
        width,
        height,
        pixels,
        palette_size: defined,
    }
}

fn rgba(pixels: &[(Rgb, u8)]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|(c, a)| [c.r, c.g, c.b, *a])
        .collect()
}

/// What the decoded image must show: each pixel blended and rounded to sixel's percent steps, or nothing.
fn expected(pixels: &[(Rgb, u8)], background: Rgb) -> Vec<Option<Rgb>> {
    pixels
        .iter()
        .map(|&(c, a)| (a != 0).then(|| as_shown(blend(c, a, background))))
        .collect()
}

const RED: Rgb = Rgb::new(255, 0, 0);
const BLUE: Rgb = Rgb::new(0, 0, 255);

#[quench_test::test]
fn sixel_of_one_opaque_pixel() {
    let out = sixel::encode(&rgba(&[(RED, 255)]), 1, 1, Rgb::BLACK).unwrap();
    assert_eq!(out, "\x1bP0;1;0q\"1;1;1;1#0;2;100;0;0#0@\x1b\\");
    let decoded = decode_sixel(&out);
    assert_eq!(decoded.pixels, vec![Some(RED)]);
}

#[quench_test::test]
fn sixel_of_two_colours_over_two_bands() {
    // 3 × 8: the top five rows red, the rest blue, so the first band holds both colours.
    let pixels: Vec<(Rgb, u8)> = (0..8)
        .flat_map(|y| (0..3).map(move |_| (if y < 5 { RED } else { BLUE }, 255)))
        .collect();
    let out = sixel::encode(&rgba(&pixels), 3, 8, Rgb::BLACK).unwrap();
    assert_eq!(
        out,
        "\x1bP0;1;0q\"1;1;3;8#0;2;100;0;0#1;2;0;0;100#0^^^$#1___-#1BBB\x1b\\"
    );
    let decoded = decode_sixel(&out);
    assert_eq!((decoded.width, decoded.height), (3, 8));
    assert_eq!(decoded.pixels, expected(&pixels, Rgb::BLACK));
    assert_eq!(decoded.palette_size, 2);
}

#[quench_test::test]
fn sixel_run_length_encodes_long_runs() {
    let pixels = vec![(Rgb::WHITE, 255); 100];
    let out = sixel::encode(&rgba(&pixels), 100, 1, Rgb::BLACK).unwrap();
    assert_eq!(out, "\x1bP0;1;0q\"1;1;100;1#0;2;100;100;100#0!100@\x1b\\");
    assert_eq!(decode_sixel(&out).pixels, expected(&pixels, Rgb::BLACK));

    // Runs of three or fewer are written out; empty sixels at the end of a row are left out.
    let mut short = vec![(Rgb::WHITE, 255); 3];
    short.extend([(Rgb::WHITE, 0); 5]);
    let out = sixel::encode(&rgba(&short), 8, 1, Rgb::BLACK).unwrap();
    assert!(out.ends_with("#0@@@\x1b\\"), "{out:?}");
    assert_eq!(decode_sixel(&out).pixels, expected(&short, Rgb::BLACK));
}

#[quench_test::test]
fn sixel_leaves_transparent_pixels_out() {
    let clear = vec![(RED, 0); 16];
    let out = sixel::encode(&rgba(&clear), 4, 4, Rgb::WHITE).unwrap();
    assert_eq!(out, "\x1bP0;1;0q\"1;1;4;4\x1b\\");
    assert!(decode_sixel(&out).pixels.iter().all(Option::is_none));

    // A hole in the middle of an opaque image stays a hole.
    let mut holed = vec![(BLUE, 255); 9];
    holed[4].1 = 0;
    let out = sixel::encode(&rgba(&holed), 3, 3, Rgb::WHITE).unwrap();
    let decoded = decode_sixel(&out);
    assert_eq!(decoded.pixels, expected(&holed, Rgb::WHITE));
    assert_eq!(decoded.pixels[4], None);
}

#[quench_test::test]
fn sixel_of_an_antialiased_formula_needs_few_colours() {
    // Black text on a white page: full coverage and four edge levels.
    let background = Rgb::WHITE;
    let levels = [0u8, 64, 128, 192, 255];
    let (width, height) = (40, 13);
    let pixels: Vec<(Rgb, u8)> = (0..width * height)
        .map(|n| (Rgb::BLACK, levels[(n * 7 + n / width) % levels.len()]))
        .collect();
    let out = sixel::encode(&rgba(&pixels), width, height, background).unwrap();
    let decoded = decode_sixel(&out);
    assert_eq!(decoded.palette_size, 4);
    assert_eq!(decoded.pixels, expected(&pixels, background));

    // Every alpha level of one colour: grey on grey has no more colours than percent has steps.
    let ramp =
        |foreground: Rgb| -> Vec<(Rgb, u8)> { (0..=255u8).map(|a| (foreground, a)).collect() };
    let grey = ramp(Rgb::new(0xdc, 0xdc, 0xdc));
    let quantized = sixel::quantize(&rgba(&grey), 16, 16, Rgb::new(0x1e, 0x1e, 0x1e), 256).unwrap();
    assert!(
        quantized.palette.len() <= 101,
        "{}",
        quantized.palette.len()
    );

    // A tinted background rounds each channel at its own step: more entries, still under 256, so the
    // colours are exact rather than median cut.
    let background = Rgb::new(0x1e, 0x1e, 0x2e);
    let ramp = ramp(Rgb::new(0xdc, 0xdc, 0xdc));
    let quantized = sixel::quantize(&rgba(&ramp), 16, 16, background, 256).unwrap();
    let distinct: std::collections::HashSet<Rgb> =
        expected(&ramp, background).into_iter().flatten().collect();
    assert_eq!(quantized.palette.len(), distinct.len());
    assert!(quantized.palette.len() < 256);
    let out = sixel::encode_quantized(&quantized);
    assert_eq!(decode_sixel(&out).pixels, expected(&ramp, background));
}

#[quench_test::test]
fn sixel_quantizes_many_colours_to_the_palette() {
    // 64 × 64 with a different colour almost everywhere: median cut down to 256 entries.
    let (width, height) = (64usize, 64usize);
    let pixels: Vec<(Rgb, u8)> = (0..width * height)
        .map(|n| {
            let (x, y) = (n % width, n / width);
            let byte = |v: usize| u8::try_from(v).unwrap();
            let c = Rgb::new(byte(x * 4), byte(y * 4), byte((x + y) * 2));
            (c, 255)
        })
        .collect();
    let quantized = sixel::quantize(&rgba(&pixels), width, height, Rgb::BLACK, 256).unwrap();
    assert_eq!(quantized.palette.len(), 256);
    let out = sixel::encode_quantized(&quantized);
    let decoded = decode_sixel(&out);
    for (n, (pixel, index)) in decoded.pixels.iter().zip(&quantized.pixels).enumerate() {
        let from_palette = quantized.palette[usize::from(index.unwrap())];
        assert_eq!(*pixel, Some(from_palette), "pixel {n}");
        let original = pixels[n].0;
        let close = |a: u8, b: u8| a.abs_diff(b) <= 24;
        assert!(
            close(original.r, from_palette.r)
                && close(original.g, from_palette.g)
                && close(original.b, from_palette.b),
            "{original:?} became {from_palette:?}"
        );
    }

    // A smaller limit is kept too.
    let few = sixel::quantize(&rgba(&pixels), width, height, Rgb::BLACK, 8).unwrap();
    assert!(few.palette.len() <= 8);
    let decoded = decode_sixel(&sixel::encode_quantized(&few));
    assert!(decoded.pixels.iter().all(Option::is_some));
}

#[quench_test::test]
fn sixel_rejects_a_buffer_of_the_wrong_size() {
    let error = sixel::encode(&[0; 7], 1, 2, Rgb::BLACK).unwrap_err();
    assert_eq!(
        error,
        SixelError::SizeMismatch {
            expected: 8,
            actual: 7
        }
    );
    assert!(error.to_string().contains("expected 8 bytes"));
    // An empty image is a valid, empty sixel.
    assert_eq!(
        sixel::encode(&[], 0, 0, Rgb::BLACK).unwrap(),
        "\x1bP0;1;0q\"1;1;0;0\x1b\\"
    );
}

#[quench_test::test]
fn sixel_blends_straight_alpha() {
    assert_eq!(blend(Rgb::BLACK, 255, Rgb::WHITE), Rgb::BLACK);
    assert_eq!(blend(Rgb::BLACK, 0, Rgb::WHITE), Rgb::WHITE);
    assert_eq!(blend(Rgb::WHITE, 128, Rgb::BLACK), Rgb::new(128, 128, 128));
    assert_eq!(as_shown(Rgb::new(128, 128, 128)), Rgb::new(128, 128, 128));
    assert_eq!(as_shown(Rgb::new(1, 2, 3)), Rgb::new(0, 3, 3));
}

// ---------------------------------------------------------------------------------------------------
// replies

const KITTY: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[6;20;10t\x1b[4;1000;1600t\
\x1b]10;rgb:dddd/dddd/dddd\x1b\\\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[?62;c";
const WEZTERM: &[u8] = b"\x1b_Gi=31;OK\x1b\\\x1b[6;18;9t\x1b[4;864;1440t\
\x1b]10;rgb:b2b2/b2b2/b2b2\x1b\\\x1b]11;rgb:0000/0000/0000\x1b\\\x1b[?65;4;6;18;22c";
const FOOT: &[u8] = b"\x1b[6;22;11t\x1b[4;1078;1914t\x1b]10;rgb:dc/dc/cc\x1b\\\
\x1b]11;rgb:11/11/11\x1b\\\x1b[?62;4;22;28c";
// xterm with window operations off: no sizes; colours BEL terminated.
const XTERM: &[u8] =
    b"\x1b]10;rgb:0000/0000/0000\x07\x1b]11;rgb:ffff/ffff/ffff\x07\x1b[?63;1;2;4;6;9;15;22c";
const VTE: &[u8] = b"\x1b[4;720;1280t\x1b]10;rgb:d0d0/cfcf/cccc\x1b\\\
\x1b]11;rgb:1717/1414/2121\x1b\\\x1b[?65;1;9c";

#[quench_test::test]
fn replies_of_kitty() {
    let replies = parse_replies(KITTY);
    assert!(replies.kitty);
    assert!(!replies.sixel());
    assert_eq!(replies.device_attributes, Some(vec![62, 0]));
    assert_eq!(replies.cell_size_px, Some((10, 20)));
    assert_eq!(replies.text_area_px, Some((1600, 1000)));
    assert_eq!(replies.foreground, Some(Rgb::new(0xdd, 0xdd, 0xdd)));
    assert_eq!(replies.background, Some(Rgb::BLACK));
}

#[quench_test::test]
fn replies_of_wezterm_foot_xterm_and_vte() {
    let wezterm = parse_replies(WEZTERM);
    assert!(wezterm.kitty && wezterm.sixel());
    assert_eq!(wezterm.cell_size_px, Some((9, 18)));
    assert_eq!(wezterm.foreground, Some(Rgb::new(0xb2, 0xb2, 0xb2)));

    let foot = parse_replies(FOOT);
    assert!(!foot.kitty && foot.sixel());
    assert_eq!(foot.cell_size_px, Some((11, 22)));
    assert_eq!(foot.foreground, Some(Rgb::new(0xdc, 0xdc, 0xcc)));
    assert_eq!(foot.background, Some(Rgb::new(0x11, 0x11, 0x11)));

    let xterm = parse_replies(XTERM);
    assert!(!xterm.kitty && xterm.sixel());
    assert_eq!(
        xterm.device_attributes,
        Some(vec![63, 1, 2, 4, 6, 9, 15, 22])
    );
    assert_eq!((xterm.cell_size_px, xterm.text_area_px), (None, None));
    assert_eq!(xterm.foreground, Some(Rgb::BLACK));
    assert_eq!(xterm.background, Some(Rgb::WHITE));

    let vte = parse_replies(VTE);
    assert!(!vte.kitty && !vte.sixel());
    assert_eq!(vte.device_attributes, Some(vec![65, 1, 9]));
    assert_eq!(vte.text_area_px, Some((1280, 720)));
    assert_eq!(vte.background, Some(Rgb::new(0x17, 0x14, 0x21)));
}

#[quench_test::test]
fn single_reply_parsers() {
    assert_eq!(parse_device_attributes(b"\x1b[?62;4c"), Some(vec![62, 4]));
    assert_eq!(parse_device_attributes(b"\x1b[>1;10;0c"), None);
    assert_eq!(parse_device_attributes(b"\x1b[?99999999999c"), None);
    assert_eq!(parse_device_attributes(b"\x1b[?6x;4c"), None);
    assert_eq!(
        parse_window_report(b"\x1b[6;16;8t"),
        Some(WindowReport::CellSize(8, 16))
    );
    assert_eq!(
        parse_window_report(b"\x1b[4;480;640t"),
        Some(WindowReport::TextArea(640, 480))
    );
    assert_eq!(parse_window_report(b"\x1b[6;0;0t"), None);
    assert_eq!(parse_window_report(b"\x1b[8;24;80t"), None);
    assert_eq!(parse_window_report(b"\x1b[6;16t"), None);

    // One to four hex digits per channel, scaled to eight bits; BEL or ST.
    let colour = |s: &[u8]| parse_osc_colour(s).map(|(_, c)| c);
    assert_eq!(
        colour(b"\x1b]11;rgb:f/8/0\x07"),
        Some(Rgb::new(255, 136, 0))
    );
    assert_eq!(
        colour(b"\x1b]11;rgb:ff/80/00\x1b\\"),
        Some(Rgb::new(255, 128, 0))
    );
    assert_eq!(
        colour(b"\x1b]11;rgb:fff/800/000\x07"),
        Some(Rgb::new(255, 128, 0))
    );
    assert_eq!(
        colour(b"\x1b]11;rgb:ffff/8080/0000\x07"),
        Some(Rgb::new(255, 128, 0))
    );
    assert_eq!(
        parse_osc_colour(b"\x1b]10;rgba:ffff/0000/0000/ffff\x1b\\"),
        Some((10, Rgb::new(255, 0, 0)))
    );
    assert_eq!(colour(b"\x1b]11;rgb:fffff/0/0\x07"), None);
    assert_eq!(colour(b"\x1b]11;rgb:ff/00\x07"), None);
    assert_eq!(colour(b"\x1b]11;rgb:gg/00/00\x07"), None);
    assert_eq!(colour(b"\x1b]11;#ff0000\x07"), None);
    assert_eq!(colour(b"\x1b]11;rgb:ff/00/00"), None);

    assert_eq!(
        parse_kitty_reply(b"\x1b_Gi=31;OK\x1b\\"),
        Some(KittyReply {
            id: Some(31),
            message: "OK".into()
        })
    );
    let error = parse_kitty_reply(b"\x1b_Gi=31,p=2;ENOENT:no such image\x1b\\").unwrap();
    assert_eq!(error.id, Some(31));
    assert!(!error.ok());
    assert_eq!(parse_kitty_reply(b"\x1b_GOK\x1b\\"), None);
}

#[quench_test::test]
fn replies_mixed_with_typing_and_errors() {
    // Keys typed while waiting, a kitty error, a DA2 reply and an unknown DCS are all skipped.
    let input = b"ab\x1b_Gi=31;EINVAL:bad\x1b\\x\x1b[>41;380;0c\x1bP1+r\x1b\\\x1b[?62;4c";
    let replies = parse_replies(input);
    assert!(!replies.kitty);
    assert!(replies.sixel());
    // A reply about another image does not count.
    assert!(!parse_replies(b"\x1b_Gi=7;OK\x1b\\").kitty);
    assert_eq!(parse_replies(b""), Replies::default());
}

#[quench_test::test]
fn an_unfinished_reply_does_not_hide_the_ones_after_it() {
    // An OSC colour reply cut short (no BEL or ST), then the device attributes.
    let replies = parse_replies(b"\x1b]11;rgb:0000/00\x1b[6;16;8t\x1b[?62;4c");
    assert!(replies.sixel());
    assert_eq!(replies.cell_size_px, Some((8, 16)));
    assert_eq!(replies.background, None);
    // The same with an unfinished kitty reply (APC) and an unfinished DCS.
    let replies = parse_replies(b"\x1b_Gi=31;O\x1b]10;rgb:ff/ff/ff\x07\x1b[?62;4c");
    assert!(!replies.kitty);
    assert_eq!(replies.foreground, Some(Rgb::WHITE));
    assert!(replies.sixel());
    assert!(parse_replies(b"\x1bP1+r544e\x1b[?62;4c").sixel());
    // CAN and SUB cancel a sequence too.
    assert!(parse_replies(b"\x1b]10;rgb\x18\x1b[?62;4c").sixel());
    assert!(parse_replies(b"\x1b_Gi=31\x1a\x1b[?62;4c").sixel());
    // Still open at the end of the input: nothing after it to find.
    let replies = parse_replies(b"\x1b[?62;4c\x1b]11;rgb:0000/0000/00");
    assert!(replies.sixel());
    assert_eq!(replies.background, None);
    // An ESC at the very end may still be the start of `ESC \\`.
    assert_eq!(parse_replies(b"\x1b]11;rgb:00/00/00\x1b").background, None);
}

#[quench_test::test]
fn eight_bit_controls_are_understood() {
    // ST as 0x9c after a 7-bit introducer.
    let replies = parse_replies(b"\x1b]11;rgb:0000/0000/0000\x9c\x1b_Gi=31;OK\x9c\x1b[?62c");
    assert_eq!(replies.background, Some(Rgb::BLACK));
    assert!(replies.kitty);
    // 8-bit introducers: CSI 0x9b, OSC 0x9d, APC 0x9f.
    let replies = parse_replies(
        b"\x9f\x47i=31;OK\x9c\x9b6;18;9t\x9d10;rgb:ff/ff/ff\x9c\x9d11;rgb:00/00/00\x07\x9b?62;4c",
    );
    assert!(replies.kitty);
    assert_eq!(replies.cell_size_px, Some((9, 18)));
    assert_eq!(replies.foreground, Some(Rgb::WHITE));
    assert_eq!(replies.background, Some(Rgb::BLACK));
    assert!(replies.sixel());
    // An 8-bit DCS is skipped like a 7-bit one.
    assert!(parse_replies(b"\x901+r\x9c\x9b?62;4c").sixel());
    // UTF-8 typed meanwhile: "ěŝÜ" holds 0x9b, 0x9d and 0x9c as continuation bytes, which are not controls.
    let typed = "ěŝÜ".as_bytes();
    assert!(typed.contains(&0x9b) && typed.contains(&0x9d) && typed.contains(&0x9c));
    let mut input = typed.to_vec();
    input.extend_from_slice(b"\x1b]11;rgb:00/00/00\x1b\\");
    input.extend_from_slice(typed);
    input.extend_from_slice(b"\x1b[?62;4c");
    let replies = parse_replies(&input);
    assert_eq!(replies.background, Some(Rgb::BLACK));
    assert!(replies.sixel());
    // ... nor inside a body: 0x9c after a non-ASCII byte does not end it.
    let mut input = b"\x1b]10;rgb:ff/ff/ff".to_vec();
    input.extend_from_slice("Ü".as_bytes());
    input.extend_from_slice(b"\x07\x1b[?62c");
    let replies = parse_replies(&input);
    assert_eq!(replies.foreground, None);
    assert_eq!(replies.device_attributes, Some(vec![62]));
}

#[quench_test::test]
fn malformed_replies_never_panic() {
    for reply in [KITTY, WEZTERM, FOOT, XTERM, VTE] {
        for end in 0..reply.len() {
            let _ = parse_replies(&reply[..end]);
            let _ = parse_replies(&reply[end..]);
        }
    }
    // Pseudo-random bytes, biased towards the bytes that matter.
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let alphabet = b"\x1b[]_P\\\x07?;:/rgbG=ic0123456789aft\x9c";
    for _ in 0..2000 {
        let input: Vec<u8> = (0..40)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                alphabet[usize::try_from(state % 64).unwrap() % alphabet.len()]
            })
            .collect();
        let _ = parse_replies(&input);
        let _ = parse_device_attributes(&input);
        let _ = parse_window_report(&input);
        let _ = parse_osc_colour(&input);
        let _ = parse_kitty_reply(&input);
    }
}

// ---------------------------------------------------------------------------------------------------
// detection

/// A terminal that answers with canned chunks, one per read, then nothing (reads of zero bytes, the
/// way a terminal in raw mode with a read timeout says nothing came).
struct Canned {
    chunks: VecDeque<io::Result<Vec<u8>>>,
    reads: usize,
}

impl Canned {
    fn new(chunks: &[&[u8]]) -> Self {
        Self {
            chunks: chunks.iter().map(|c| Ok(c.to_vec())).collect(),
            reads: 0,
        }
    }
}

impl Read for Canned {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        match self.chunks.pop_front() {
            None => Ok(0),
            Some(Err(error)) => Err(error),
            Some(Ok(chunk)) => {
                let n = chunk.len().min(buf.len());
                buf[..n].copy_from_slice(&chunk[..n]);
                if n < chunk.len() {
                    self.chunks.push_front(Ok(chunk[n..].to_vec()));
                }
                Ok(n)
            }
        }
    }
}

/// A clock that moves on 10 ms each time it is read.
fn ticking() -> impl FnMut() -> Duration {
    let mut now = Duration::ZERO;
    move || {
        let then = now;
        now += Duration::from_millis(10);
        then
    }
}

fn run_probe(input: &mut Canned) -> (Replies, Vec<u8>) {
    let mut written = Vec::new();
    let replies = probe(
        &mut written,
        input,
        Duration::from_millis(200),
        &mut ticking(),
    )
    .unwrap();
    (replies, written)
}

#[quench_test::test]
fn probe_sends_the_queries_with_device_attributes_last() {
    let (_, written) = run_probe(&mut Canned::new(&[]));
    assert_eq!(written, QUERIES.as_bytes());
    assert!(QUERIES.starts_with(kitty::QUERY));
    assert!(QUERIES.ends_with("\x1b[c"));
    for query in ["\x1b[16t", "\x1b[14t", "\x1b]10;?\x1b\\", "\x1b]11;?\x1b\\"] {
        assert!(QUERIES.contains(query));
    }
}

#[quench_test::test]
fn probe_of_a_kitty_and_sixel_terminal() {
    // The answers come in pieces, cut in the middle of sequences.
    let (head, tail) = WEZTERM.split_at(30);
    let (middle, last) = tail.split_at(40);
    let mut input = Canned::new(&[head, b"", middle, last, b"\x1b[?1;2c"]);
    let (replies, _) = run_probe(&mut input);
    assert!(replies.kitty && replies.sixel());
    // Reading stopped at the device attributes reply: what came after is still unread.
    assert_eq!(input.chunks.len(), 1);
    let capabilities = Capabilities::from_replies(&replies, Some((160, 48)));
    assert_eq!(capabilities.protocol(), Some(Protocol::Kitty));
    assert_eq!(capabilities.cell_size_px, Some((9, 18)));
    assert_eq!(capabilities.background, Some(Rgb::BLACK));
}

#[quench_test::test]
fn probe_of_a_sixel_only_terminal() {
    let mut input = Canned::new(&[FOOT]);
    let (replies, _) = run_probe(&mut input);
    let capabilities = Capabilities::from_replies(&replies, None);
    assert_eq!(capabilities.protocol(), Some(Protocol::Sixel));
    assert!(!capabilities.kitty);
    assert_eq!(capabilities.cell_size_px, Some((11, 22)));
    assert_eq!(input.reads, 1);
}

#[quench_test::test]
fn probe_of_a_terminal_that_only_answers_device_attributes() {
    let (replies, _) = run_probe(&mut Canned::new(&[b"\x1b[?62c"]));
    let capabilities = Capabilities::from_replies(&replies, Some((80, 24)));
    assert_eq!(
        capabilities,
        Capabilities {
            kitty: false,
            sixel: false,
            ..Capabilities::NONE
        }
    );
    assert_eq!(capabilities.protocol(), None);
}

/// A terminal in raw mode with a read timeout of a tenth of a second that never answers: each read
/// returns no bytes after 100 ms (moving the shared clock on).
struct Silent {
    now: Rc<Cell<Duration>>,
    reads: usize,
}

impl Read for Silent {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        self.now.set(self.now.get() + Duration::from_millis(100));
        Ok(0)
    }
}

/// An input at its end: every read returns no bytes at once (or, with `would_block`, fails so).
struct Ended {
    reads: usize,
    would_block: bool,
}

impl Read for Ended {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        if self.would_block {
            Err(io::ErrorKind::WouldBlock.into())
        } else {
            Ok(0)
        }
    }
}

#[quench_test::test]
fn probe_gives_up_when_nothing_answers() {
    // The terminal's read timeout paces the loop: 0 and 100 ms read, at 200 ms it stops, no sleeping.
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut input = Silent {
        now: Rc::clone(&now),
        reads: 0,
    };
    let mut calls = 0u32;
    let mut clock = || {
        calls += 1;
        now.get()
    };
    let real = Instant::now();
    let replies = probe(
        &mut Vec::new(),
        &mut input,
        Duration::from_millis(200),
        &mut clock,
    )
    .unwrap();
    assert_eq!(replies, Replies::default());
    assert_eq!(input.reads, 2);
    assert_eq!(calls, 3);
    assert!(real.elapsed() < Duration::from_millis(100));

    // Answers without the sentinel are kept when the time runs out.
    let (replies, _) = run_probe(&mut Canned::new(&[b"\x1b[6;16;8t"]));
    assert_eq!(replies.cell_size_px, Some((8, 16)));
    assert_eq!(replies.device_attributes, None);
}

#[quench_test::test]
fn probe_at_the_end_of_input_backs_off_instead_of_spinning() {
    for would_block in [false, true] {
        let mut input = Ended {
            reads: 0,
            would_block,
        };
        let start = Instant::now();
        let replies = probe(
            &mut Vec::new(),
            &mut input,
            Duration::from_millis(100),
            &mut || start.elapsed(),
        )
        .unwrap();
        let took = start.elapsed();
        assert_eq!(replies, Replies::default());
        // About one read per 10 ms, not millions; and it ends by the timeout.
        assert!(
            input.reads >= 2 && input.reads <= 11,
            "{} reads",
            input.reads
        );
        assert!(took >= Duration::from_millis(100), "{took:?}");
        assert!(took < Duration::from_millis(1000), "{took:?}");
    }
    // With a clock that does not move by itself, the reads at once still end by the timeout:
    // each one is followed by a sleep, and the ticking clock moves 20 ms per round.
    let mut input = Ended {
        reads: 0,
        would_block: false,
    };
    probe(
        &mut Vec::new(),
        &mut input,
        Duration::from_millis(200),
        &mut ticking(),
    )
    .unwrap();
    assert_eq!(input.reads, 10);
}

/// An output that is gone.
struct Broken;
impl Write for Broken {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("closed"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[quench_test::test]
fn probe_retries_interruptions_and_reports_read_errors() {
    let mut input = Canned::new(&[]);
    input
        .chunks
        .push_back(Err(io::ErrorKind::Interrupted.into()));
    input
        .chunks
        .push_back(Err(io::ErrorKind::WouldBlock.into()));
    input.chunks.push_back(Ok(b"\x1b[?62;4c".to_vec()));
    assert!(run_probe(&mut input).0.sixel());

    let mut input = Canned::new(&[b"\x1b[6;16;8t"]);
    input
        .chunks
        .push_back(Err(io::ErrorKind::UnexpectedEof.into()));
    let (replies, _) = run_probe(&mut input);
    assert_eq!(replies.cell_size_px, Some((8, 16)));

    let mut input = Canned::new(&[]);
    input.chunks.push_back(Err(io::Error::other("gone")));
    let result = probe(
        &mut Vec::new(),
        &mut input,
        Duration::from_millis(200),
        &mut ticking(),
    );
    assert!(result.is_err());

    let result = probe(
        &mut Broken,
        &mut Canned::new(&[]),
        Duration::ZERO,
        &mut ticking(),
    );
    assert!(result.is_err());
}

#[quench_test::test]
fn the_cell_size_is_derived_from_the_text_area() {
    let replies = parse_replies(VTE);
    let capabilities = Capabilities::from_replies(&replies, Some((128, 36)));
    assert_eq!(capabilities.cell_size_px, Some((10, 20)));
    // Without the grid, or with a nonsense one, there is nothing to divide by.
    assert_eq!(
        Capabilities::from_replies(&replies, None).cell_size_px,
        None
    );
    assert_eq!(
        Capabilities::from_replies(&replies, Some((0, 36))).cell_size_px,
        None
    );
    assert_eq!(
        Capabilities::from_replies(&replies, Some((5000, 5000))).cell_size_px,
        None
    );
    // A reported cell size wins over the derived one.
    let kitty = Capabilities::from_replies(&parse_replies(KITTY), Some((1, 1)));
    assert_eq!(kitty.cell_size_px, Some((10, 20)));
}

fn environment(term: &str, graphics: Option<&str>, multiplexer: bool) -> Environment {
    Environment {
        term: Some(term.to_string()),
        graphics: graphics.map(str::to_string),
        multiplexer,
        terminal: true,
    }
}

#[quench_test::test]
fn the_environment_decides_whether_to_ask() {
    let ask = Plan {
        probe: true,
        forced: None,
    };
    assert_eq!(plan(&environment("xterm-kitty", None, false)), ask);
    // No terminal, a dumb one, no TERM at all: nothing is sent.
    let not_a_terminal = Environment {
        terminal: false,
        ..environment("xterm-256color", None, false)
    };
    assert!(!plan(&not_a_terminal).probe);
    assert!(!plan(&environment("dumb", None, false)).probe);
    assert!(!plan(&environment("", None, false)).probe);
    let no_term = Environment {
        term: None,
        ..environment("x", None, false)
    };
    assert!(!plan(&no_term).probe);
    // Inside tmux or screen: nothing is ever sent, and no graphics unless forced.
    let tmux = plan(&environment("tmux-256color", None, true));
    assert!(!tmux.probe);
    assert_eq!(resolve(tmux, None, None), Capabilities::NONE);
    // A forced protocol is reported without measuring: no cell size, no colours.
    for (value, protocol) in [("kitty", Protocol::Kitty), ("sixel", Protocol::Sixel)] {
        let forced = plan(&environment("tmux-256color", Some(value), true));
        assert!(!forced.probe);
        let capabilities = resolve(forced, None, None);
        assert_eq!(capabilities.protocol(), Some(protocol));
        assert_eq!(capabilities.cell_size_px, None);
        assert_eq!(capabilities.text_area_px, None);
        assert_eq!(capabilities.foreground, None);
        assert_eq!(capabilities.background, None);
    }
    let screen = plan(&environment("screen", Some("none"), true));
    assert!(!screen.probe);
    assert_eq!(resolve(screen, None, None), Capabilities::NONE);
}

#[quench_test::test]
fn the_override_forces_the_answer() {
    assert_eq!(Forced::parse("none"), Some(Forced::None));
    assert_eq!(Forced::parse(" Kitty "), Some(Forced::Kitty));
    assert_eq!(Forced::parse("SIXEL"), Some(Forced::Sixel));
    assert_eq!(Forced::parse("iterm"), None);

    let wezterm = parse_replies(WEZTERM);
    let none = plan(&environment("xterm", Some("none"), false));
    assert!(!none.probe);
    assert_eq!(resolve(none, Some(&wezterm), None), Capabilities::NONE);

    let sixel = plan(&environment("xterm", Some("sixel"), false));
    let capabilities = resolve(sixel, Some(&wezterm), None);
    assert_eq!(capabilities.protocol(), Some(Protocol::Sixel));
    assert!(!capabilities.kitty);
    // What was measured stays.
    assert_eq!(capabilities.cell_size_px, Some((9, 18)));

    let kitty = plan(&environment("xterm", Some("kitty"), false));
    let capabilities = resolve(kitty, Some(&parse_replies(VTE)), None);
    assert!(capabilities.kitty && !capabilities.sixel);

    // An unknown value is ignored: the terminal's answer counts.
    let unknown = plan(&environment("xterm", Some("iterm2"), false));
    assert_eq!(unknown.forced, None);
    assert_eq!(
        resolve(unknown, Some(&wezterm), None).protocol(),
        Some(Protocol::Kitty)
    );
}

#[cfg(all(unix, feature = "tui"))]
#[quench_test::test]
fn detect_without_a_terminal_sends_nothing() {
    use quench_cli::graphics::detect::detect_in;
    let forced = Environment {
        terminal: false,
        ..environment("xterm", Some("sixel"), false)
    };
    let capabilities = detect_in(&forced, Duration::from_millis(50));
    assert_eq!(capabilities.protocol(), Some(Protocol::Sixel));
    assert_eq!(capabilities.cell_size_px, None);
    let plain = Environment {
        terminal: false,
        ..environment("xterm", None, false)
    };
    assert_eq!(
        detect_in(&plain, Duration::from_millis(50)),
        Capabilities::NONE
    );
}
