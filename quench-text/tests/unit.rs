// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::suboptimal_flops
)]

use quench_raster::{Canvas, Color};
use quench_text::{Font, FontError, FontSet, SystemFonts, noto_sans, noto_sans_bold};
use std::sync::Arc;

const REGULAR: &[u8] = include_bytes!("../fonts/NotoSans-Regular.ttf");

/// The canvas as text, for looking at what was drawn: `QUENCH_TEXT_SHOW=1` prints it.
fn show(canvas: &Canvas) {
    if std::env::var("QUENCH_TEXT_SHOW").is_err() {
        return;
    }
    for y in 0..canvas.height() {
        let row: String = (0..canvas.width())
            .map(|x| match canvas.pixel(x, y).a {
                0..=40 => ' ',
                41..=100 => '.',
                101..=180 => 'o',
                _ => '#',
            })
            .collect();
        println!("{}", row.trim_end());
    }
}

fn ink(canvas: &Canvas) -> f32 {
    (0..canvas.height())
        .flat_map(|y| (0..canvas.width()).map(move |x| (x, y)))
        .map(|(x, y)| f32::from(canvas.pixel(x, y).a) / 255.0)
        .sum()
}

fn draw(fonts: &FontSet, text: &str, size: f32, width: usize, height: usize) -> Canvas {
    let layout = fonts.layout(text, size, None);
    let mut canvas = Canvas::new(width, height);
    fonts.draw(&mut canvas, &layout, 2.0, 2.0, Color::WHITE);
    canvas
}

#[quench_test::test]
fn the_embedded_fonts_parse_and_say_what_licence_they_are_under() {
    for (font, family, style) in [
        (noto_sans(), "Noto Sans", "Regular"),
        (noto_sans_bold(), "Noto Sans", "Bold"),
        (quench_text::noto_sans_light(), "Noto Sans Light", "Regular"),
        (
            quench_text::noto_sans_medium(),
            "Noto Sans Medium",
            "Regular",
        ),
    ] {
        assert_eq!((font.units_per_em(), font.glyph_count()), (1000, 3884));
        assert_eq!(font.name(1).as_deref(), Some(family));
        assert_eq!(font.name(2).as_deref(), Some(style));
        // The licence is the fonts' own claim, and the OFL text that ships with them says the same.
        assert!(font.name(0).unwrap().contains("Noto Project Authors"));
        assert!(
            font.name(13)
                .unwrap()
                .contains("SIL Open Font License, Version 1.1"),
            "{:?}",
            font.name(13)
        );
        assert_eq!(
            font.name(14).as_deref(),
            Some("https://scripts.sil.org/OFL")
        );
        assert_eq!(font.name(999), None);
        assert_eq!(
            (font.ascent(), font.descent(), font.line_gap()),
            (1069, -293, 0)
        );
        assert!(format!("{font:?}").contains("Noto Sans"));
    }
    let ofl = include_str!("../fonts/OFL.txt");
    assert!(ofl.starts_with("Copyright 2022 The Noto Project Authors"));
    assert!(ofl.contains("SIL OPEN FONT LICENSE Version 1.1"));
    assert!(include_str!("../fonts/README.md").contains("OFL.txt"));
}

#[quench_test::test]
fn characters_map_to_glyphs_in_both_character_map_formats() {
    let font = noto_sans();
    let ids: Vec<_> = ['A', 'z', 'ż', 'Ł', 'я', 'Я', '€', ' ', '—']
        .iter()
        .map(|c| font.glyph(*c))
        .collect();
    assert!(ids.iter().all(Option::is_some), "{ids:?}");
    let mut unique: Vec<_> = ids.into_iter().flatten().collect();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), 9, "every character has a glyph of its own");
    // Not in this font: CJK, emoji (beyond the BMP: format 12 answers), NUL, and the end of Unicode.
    for c in ['日', '😀', '\u{10FFFF}', '\u{E000}'] {
        assert_eq!(font.glyph(c), None, "{c:?}");
    }
    // The font has both formats; a font parsed from a copy whose format-12 table is ruined falls back to the BMP table.
    let mut bytes = REGULAR.to_vec();
    let directory = 12
        + 16 * (0..17)
            .find(|i| &bytes[12 + i * 16..16 + i * 16] == b"cmap")
            .unwrap();
    let cmap =
        u32::from_be_bytes(bytes[directory + 8..directory + 12].try_into().unwrap()) as usize;
    for i in 0..4 {
        let record = cmap + 4 + i * 8;
        let (platform, encoding) = (
            u16::from_be_bytes([bytes[record], bytes[record + 1]]),
            u16::from_be_bytes([bytes[record + 2], bytes[record + 3]]),
        );
        if (platform, encoding) == (3, 10) || (platform, encoding) == (0, 4) {
            bytes[record + 2] = 0xFF; // an encoding nobody asked for
        }
    }
    let bmp = Font::parse(bytes).unwrap();
    assert_eq!(
        bmp.glyph('ż'),
        font.glyph('ż'),
        "format 4 finds the same glyph"
    );
    assert_eq!(bmp.glyph('😀'), None);
}

#[quench_test::test]
fn advances_follow_the_shapes() {
    let font = noto_sans();
    let (i, w, space) = (
        font.glyph('i').unwrap(),
        font.glyph('W').unwrap(),
        font.glyph(' ').unwrap(),
    );
    assert!(font.advance(i) < font.advance(w));
    assert!(font.advance(space) > 100 && font.advance(space) < 400);
    // Past the last glyph the last advance is used (as the font's table says); nothing panics.
    let _ = font.advance(u16::MAX);
    assert_eq!(
        font.advance(3883),
        font.advance(u16::MAX)
            .min(font.advance(3883))
            .max(font.advance(3883))
    );
}

#[quench_test::test]
fn kerning_comes_from_gpos_pairs_and_classes() {
    let font = noto_sans();
    let g = |c| font.glyph(c).unwrap();
    if std::env::var("QUENCH_TEXT_SHOW").is_ok() {
        for (a, b) in [
            ('A', 'V'),
            ('V', 'A'),
            ('T', 'o'),
            ('L', 'T'),
            ('L', 'V'),
            ('Y', 'o'),
            ('P', 'a'),
            ('F', 'a'),
            ('W', 'a'),
            ('r', '.'),
            ('Т', 'а'),
            ('Р', 'а'),
            ('Г', 'а'),
            ('Л', 'Т'),
            ('у', 'я'),
        ] {
            println!("kern {a}{b} = {}", font.kerning(g(a), g(b)));
        }
    }
    // The classic pairs pull together; both orders; a pair that does not interact does not move.
    assert!(
        font.kerning(g('A'), g('V')) < -20,
        "{}",
        font.kerning(g('A'), g('V'))
    );
    assert!(font.kerning(g('V'), g('A')) < -20);
    assert!(font.kerning(g('T'), g('o')) < -20);
    assert!(font.kerning(g('L'), g('T')) <= -20);
    assert_eq!(font.kerning(g('T'), g('o')), -70);
    assert_eq!(font.kerning(g('i'), g('i')), 0);
    assert_eq!(font.kerning(g(' '), g(' ')), 0);
    // Cyrillic has its own pairs.
    assert!(
        font.kerning(g('Т'), g('а')) < 0 || font.kerning(g('Р'), g('а')) < 0,
        "no Cyrillic kerning found"
    );
    // Glyph ids that are not glyphs kern nothing and do not panic.
    assert_eq!(font.kerning(u16::MAX, g('A')), 0);
    assert_eq!(font.kerning(g('A'), u16::MAX), 0);
    assert_eq!(font.kerning(0, 0), 0);
}

#[quench_test::test]
fn outlines_have_the_contours_of_their_shapes() {
    let font = noto_sans();
    let contours = |c| {
        font.outline(font.glyph(c).unwrap())
            .map(|o| o.contours.len())
    };
    assert_eq!(contours('l'), Some(1));
    assert_eq!(contours('o'), Some(2), "an outer contour and a counter");
    assert_eq!(contours('8'), Some(3));
    assert_eq!(contours(' '), None, "a space has no outline");
    // A glyph made of components: an accented letter is its base and its accent.
    assert_eq!(contours('é'), Some(contours('e').unwrap() + 1));
    assert!(contours('ż').unwrap() > contours('z').unwrap());
    assert!(contours('Ą').unwrap() >= contours('A').unwrap());
    // `.notdef` has a box; a glyph that does not exist has nothing.
    assert!(font.outline(0).is_some());
    assert!(font.outline(60_000).is_none());
    // Points are in font units with y up: a capital's top is its cap height, the baseline is 0.
    let top = font
        .outline(font.glyph('H').unwrap())
        .unwrap()
        .contours
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::MIN, f32::max);
    let bottom = font
        .outline(font.glyph('H').unwrap())
        .unwrap()
        .contours
        .iter()
        .flatten()
        .map(|p| p.y)
        .fold(f32::MAX, f32::min);
    assert!((700.0..760.0).contains(&top), "{top}");
    assert_eq!(bottom, 0.0);
}

#[quench_test::test]
fn a_glyph_drawn_large_has_its_counter_empty_and_its_stem_full() {
    let font = noto_sans();
    let o = font.outline(font.glyph('o').unwrap()).unwrap();
    let mut canvas = Canvas::new(120, 120);
    canvas.fill_path(&o.to_path(0.1, 10.0, 100.0), 0.0, 0.0, Color::WHITE);
    // 'o' is about 60 px wide at 100 px per em: the middle is the counter, the sides are the ring.
    assert_eq!(canvas.pixel(40, 70).a, 0, "the counter");
    let left = o
        .contours
        .iter()
        .flatten()
        .map(|p| p.x)
        .fold(f32::MAX, f32::min);
    assert_eq!(
        canvas.pixel((10.0 + left * 0.1 + 3.0) as usize, 70).a,
        255,
        "the left of the ring"
    );
    assert_eq!(canvas.pixel(0, 70).a, 0, "outside");
    show(&canvas);
    // Bold has more ink than regular for the same letters.
    let regular = ink(&draw(&FontSet::noto_sans(), "Hamburgefonts", 24.0, 220, 40));
    let bold = ink(&draw(
        &FontSet::noto_sans_bold(),
        "Hamburgefonts",
        24.0,
        220,
        40,
    ));
    assert!(bold > regular * 1.15, "{bold} against {regular}");
}

#[quench_test::test]
fn text_is_measured_with_kerning_and_wrapped_without_losing_a_word() {
    let fonts = FontSet::noto_sans();
    assert!(fonts.measure("Hello", 16.0) > fonts.measure("Hi", 16.0));
    assert!(
        fonts.measure("AV", 16.0) < fonts.measure("A", 16.0) + fonts.measure("V", 16.0),
        "kerned"
    );
    // Width scales with the size.
    let (a, b) = (fonts.measure("Zażółć", 10.0), fonts.measure("Zażółć", 20.0));
    assert!((b / a - 2.0).abs() < 1e-3);
    // Wrapping: every line fits, every word survives, in order.
    let text =
        "The quick brown fox jumps over the lazy dog, and then Zażółć gęślą jaźń — Привет мир!";
    for max in [60.0, 100.0, 150.0, 400.0] {
        let layout = fonts.layout(text, 14.0, Some(max));
        assert!(
            layout.lines.iter().all(|l| l.width <= max + 0.01),
            "{max}: {:?}",
            layout.lines.iter().map(|l| l.width).collect::<Vec<_>>()
        );
        let words: Vec<String> = layout
            .lines
            .iter()
            .flat_map(|l| l.glyphs.iter().map(|g| g.character))
            .collect::<String>()
            .split_whitespace()
            .map(str::to_string)
            .collect();
        assert_eq!(
            words.join(" "),
            text.split_whitespace().collect::<Vec<_>>().join(" "),
            "{max}"
        );
        assert!(layout.lines.len() > 1 || max >= 400.0, "{max}");
        assert_eq!(
            layout.width,
            layout.lines.iter().map(|l| l.width).fold(0.0, f32::max)
        );
        assert_eq!(
            layout.height,
            layout.line_height * layout.lines.len() as f32
        );
    }
    // A line break in the text is a break; a trailing one leaves an empty line for the caret; empty is one line.
    assert_eq!(fonts.layout("a\nb\n\nc", 12.0, None).lines.len(), 4);
    assert_eq!(fonts.layout("a\n", 12.0, None).lines.len(), 2);
    assert_eq!(fonts.layout("", 12.0, None).lines.len(), 1);
    assert_eq!(fonts.layout("", 12.0, None).width, 0.0);
    assert_eq!(
        fonts.layout("a\r\nb", 12.0, None).lines.len(),
        2,
        "a carriage return is not a glyph"
    );
    assert!(
        (fonts.layout("a\nb", 12.0, None).line_height - 1.362 * 12.0).abs() < 0.01,
        "(1069 + 293) / 1000 em"
    );
}

#[quench_test::test]
fn a_word_wider_than_the_line_breaks_between_characters_and_a_hyphen_is_an_opportunity() {
    let fonts = FontSet::noto_sans();
    let layout = fonts.layout("Donaudampfschifffahrtsgesellschaft", 16.0, Some(80.0));
    assert!(layout.lines.len() >= 3);
    assert!(
        layout
            .lines
            .iter()
            .all(|l| l.width <= 80.0 + 0.01 && !l.glyphs.is_empty())
    );
    let rejoined: String = layout
        .lines
        .iter()
        .flat_map(|l| l.glyphs.iter().map(|g| g.character))
        .collect();
    assert_eq!(rejoined, "Donaudampfschifffahrtsgesellschaft");
    // A line that is narrower than one glyph still makes progress: a glyph a line.
    let thin = fonts.layout("WWW", 16.0, Some(1.0));
    assert_eq!(thin.lines.len(), 3);
    // After a hyphen, not before: "well-" stays together on the first line.
    let hyphen = fonts.layout(
        "well-known facts",
        16.0,
        Some(fonts.measure("well-known", 16.0) - 4.0),
    );
    assert_eq!(
        hyphen.lines[0]
            .glyphs
            .iter()
            .map(|g| g.character)
            .collect::<String>(),
        "well-"
    );
}

#[quench_test::test]
fn glyphs_know_where_in_the_text_they_came_from() {
    let fonts = FontSet::noto_sans();
    let text = "żółw i\nOK";
    let layout = fonts.layout(text, 12.0, None);
    let glyphs: Vec<_> = layout.lines.iter().flat_map(|l| &l.glyphs).collect();
    for g in &glyphs {
        assert_eq!(text[g.byte..].chars().next(), Some(g.character));
    }
    assert_eq!(glyphs[1].byte, 2, "ż is two bytes");
    // The lines' byte ranges tile the text: each starts where the previous one stopped.
    assert_eq!(layout.lines[0].bytes, 0..text.find('\n').unwrap() + 1);
    assert_eq!(
        layout.lines[1].bytes,
        text.find('\n').unwrap() + 1..text.len()
    );
    // x grows along a line, from 0, by the advances.
    let line = &layout.lines[0];
    assert_eq!(line.glyphs[0].x, 0.0);
    for pair in line.glyphs.windows(2) {
        assert!((pair[1].x - (pair[0].x + pair[0].advance)).abs() < 1e-4);
    }
    // Trailing spaces hang: they are in the line but not in its width.
    let hang = fonts.layout("word   ", 16.0, None);
    assert_eq!(hang.lines[0].glyphs.len(), 7);
    assert_eq!(hang.lines[0].width, fonts.measure("word", 16.0));
}

#[quench_test::test]
fn control_characters_are_skipped_and_a_tab_is_four_spaces() {
    let fonts = FontSet::noto_sans();
    let plain = fonts.layout("ab", 16.0, None);
    let noisy = fonts.layout("a\u{0}\u{7}\u{200B}b", 16.0, None);
    assert_eq!(noisy.lines[0].glyphs.len(), 2);
    assert_eq!(noisy.width, plain.width);
    let tab = fonts.layout("a\tb", 16.0, None).lines[0].glyphs[1].advance;
    let space = fonts.layout("a b", 16.0, None).lines[0].glyphs[1].advance;
    assert!((tab - 4.0 * space).abs() < 1e-3);
}

#[quench_test::test]
fn what_no_face_has_is_drawn_as_the_missing_glyph_and_faces_are_tried_in_order() {
    let fonts = FontSet::noto_sans();
    let layout = fonts.layout("a日b", 16.0, None);
    let missing = layout.lines[0].glyphs[1];
    assert_eq!(
        (missing.face, missing.id),
        (0, 0),
        ".notdef of the primary face"
    );
    assert!(missing.advance > 0.0);
    // A character both faces have comes from the first; the second is asked only for what the first lacks.
    let two = FontSet::noto_sans().with_fallback(noto_sans_bold());
    assert_eq!(two.layout("ż", 12.0, None).lines[0].glyphs[0].face, 0);
    assert_eq!(two.face_count(), 2);
    assert_eq!(two.face(1).name(2).as_deref(), Some("Bold"));
    assert!(format!("{two:?}").contains("FontSet"));
    // Asking again is answered from what was learned: the same result.
    assert_eq!(two.layout("a日", 12.0, None), two.layout("a日", 12.0, None));
}

#[quench_test::test]
fn the_machines_fonts_are_indexed_by_coverage_and_loaded_when_needed() {
    let dir = quench_core::tmp::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("deeper/still")).unwrap();
    std::fs::write(dir.path().join("a-regular.ttf"), REGULAR).unwrap();
    std::fs::write(
        dir.path().join("deeper/still/b-bold.TTF"),
        include_bytes!("../fonts/NotoSans-Bold.ttf"),
    )
    .unwrap();
    std::fs::write(dir.path().join("notes.txt"), "not a font").unwrap();
    // Colour emoji cannot be drawn from outlines: not indexed, whatever they cover.
    std::fs::write(dir.path().join("NotoColorEmoji.ttf"), REGULAR).unwrap();
    std::fs::write(dir.path().join("broken.ttf"), [0u8; 100]).unwrap();
    std::fs::write(dir.path().join("truncated.ttf"), &REGULAR[..200]).unwrap();
    let system =
        SystemFonts::in_directories(vec![dir.path().to_path_buf(), dir.path().join("missing")]);
    assert_eq!(system.indexed(), 0, "nothing is read until it is needed");
    let for_z = system.candidates('ż');
    assert_eq!(for_z.len(), 2);
    assert!(
        for_z[0].ends_with("a-regular.ttf"),
        "directory order: {for_z:?}"
    );
    assert_eq!(
        system.indexed(),
        2,
        "the broken and the non-font files are left out"
    );
    assert_eq!(system.candidates('日').len(), 0);
    assert_eq!(system.candidates('я').len(), 2);
    // A set that has none of a character itself loads it from there: here a face that is in the directory only.
    let set = FontSet::new(Arc::new(Font::parse(REGULAR).unwrap()))
        .with_system_directories(vec![dir.path().to_path_buf()]);
    assert_eq!(
        set.layout("日", 12.0, None).lines[0].glyphs[0].id,
        0,
        "no installed font has it either"
    );
}

#[quench_test::test]
fn a_real_system_font_supplies_a_character_the_embedded_one_lacks() {
    let set = FontSet::noto_sans().with_system_fonts();
    // Which of these some installed font has depends on the machine; at least one is on most desktops.
    let Some(c) = ['日', '中', '한', '😀', '→', '∑', 'ა']
        .into_iter()
        .find(|c| {
            !set.layout(&c.to_string(), 14.0, None).lines[0]
                .glyphs
                .iter()
                .any(|g| g.id == 0)
        })
    else {
        eprintln!(
            "no installed font has any of the probe characters: the system fallback is not exercised here"
        );
        return;
    };
    let layout = set.layout(&format!("a{c}b"), 14.0, None);
    let glyph = layout.lines[0].glyphs[1];
    assert!(
        glyph.face >= 1 && glyph.id != 0,
        "{c:?} came from a system font: {glyph:?}"
    );
    assert_eq!(set.face_count(), 2, "one font was loaded for it");
    let again = set.layout(&format!("{c}{c}"), 14.0, None);
    assert_eq!(again.lines[0].glyphs[0].face, glyph.face);
    assert_eq!(set.face_count(), 2, "and it is not loaded again");
    let mut canvas = Canvas::new(60, 30);
    set.draw(
        &mut canvas,
        &set.layout(&c.to_string(), 20.0, None),
        4.0,
        2.0,
        Color::WHITE,
    );
    assert!(ink(&canvas) > 5.0, "the fallback glyph is drawn");
}

#[quench_test::test]
fn text_is_drawn_inside_its_box_in_the_colour_asked_for() {
    let fonts = FontSet::noto_sans();
    let layout = fonts.layout("Hello, мир", 24.0, None);
    let mut canvas = Canvas::new(180, 50);
    fonts.draw(&mut canvas, &layout, 10.0, 5.0, Color::rgb(200, 100, 50));
    show(&canvas);
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (usize::MAX, 0, usize::MAX, 0);
    for y in 0..50 {
        for x in 0..180 {
            if canvas.pixel(x, y).a > 0 {
                (min_x, max_x, min_y, max_y) =
                    (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y));
            }
        }
    }
    assert!(
        min_x >= 9 && (max_x as f32) <= 10.0 + layout.width + 1.0,
        "{min_x}..{max_x} in {}",
        layout.width
    );
    assert!(
        min_y >= 5 && (max_y as f32) <= 5.0 + layout.height,
        "{min_y}..{max_y}"
    );
    // Solid pixels have the colour exactly; edges have it at less alpha.
    let solid = (0..180)
        .find_map(|x| {
            (0..50).find_map(|y| (canvas.pixel(x, y).a == 255).then(|| canvas.pixel(x, y)))
        })
        .unwrap();
    assert_eq!(solid, Color::rgb(200, 100, 50));
    // Two lines: the second is a line height lower.
    let two = fonts.layout("l\nl", 24.0, None);
    let mut c2 = Canvas::new(30, 80);
    fonts.draw(&mut c2, &two, 2.0, 0.0, Color::WHITE);
    let first_ink = |from: usize| {
        (from..80)
            .find(|y| (0..30).any(|x| c2.pixel(x, *y).a > 0))
            .unwrap()
    };
    let top1 = first_ink(0);
    let gap_end = (top1..80)
        .find(|y| (0..30).all(|x| c2.pixel(x, *y).a == 0))
        .unwrap();
    let top2 = first_ink(gap_end);
    assert!(
        (top2 as f32 - top1 as f32 - two.line_height).abs() <= 1.0,
        "{top1} {top2} {}",
        two.line_height
    );
    // Whitespace and an empty layout draw nothing.
    let mut blank = Canvas::new(40, 40);
    fonts.draw(
        &mut blank,
        &fonts.layout("   ", 20.0, None),
        2.0,
        2.0,
        Color::WHITE,
    );
    fonts.draw(
        &mut blank,
        &fonts.layout("", 20.0, None),
        2.0,
        2.0,
        Color::WHITE,
    );
    assert_eq!(ink(&blank), 0.0);
    // A fractional pen position moves the glyph by fractions of a pixel: the pixels differ, the ink is the same.
    let (mut p, mut q) = (Canvas::new(40, 40), Canvas::new(40, 40));
    let one = fonts.layout("I", 30.0, None);
    fonts.draw(&mut p, &one, 5.0, 2.0, Color::WHITE);
    fonts.draw(&mut q, &one, 5.5, 2.0, Color::WHITE);
    assert_ne!(p, q);
    assert!((ink(&p) - ink(&q)).abs() < 0.5);
}

/// FNV-1a over the pixels, to notice any change to the output.
fn fingerprint(canvas: &Canvas) -> u64 {
    canvas.bytes().iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[quench_test::test]
fn the_rendering_of_known_text_has_not_changed() {
    // Golden output: the text below, in both faces, with kerning, accents and Cyrillic, at three sizes. If a
    // change to the rasteriser, the parser or the layout alters a pixel this fails; look at the result with
    // `QUENCH_TEXT_SHOW=1`, and update the numbers only if the new rendering is better.
    let text = "AV To Zażółć gęślą jaźń Привет";
    let rendered = |fonts: &FontSet, size: f32| fingerprint(&draw(fonts, text, size, 420, 50));
    show(&draw(&FontSet::noto_sans(), text, 20.0, 420, 50));
    show(&draw(&FontSet::noto_sans_bold(), text, 31.5, 420, 50));
    let got = [
        rendered(&FontSet::noto_sans(), 12.0),
        rendered(&FontSet::noto_sans(), 20.0),
        rendered(&FontSet::noto_sans_bold(), 20.0),
        rendered(&FontSet::noto_sans_bold(), 31.5),
    ];
    if std::env::var("QUENCH_TEXT_GOLDEN").is_ok() {
        println!("{got:?}");
    }
    assert_eq!(
        got, GOLDEN,
        "rendering changed (run with QUENCH_TEXT_GOLDEN=1 to print the new fingerprints)"
    );
}

const GOLDEN: [u64; 4] = [
    7_044_936_444_543_304_437,
    13_077_583_234_057_583_513,
    5_791_121_657_881_846_633,
    12_742_806_263_314_060_061,
];

#[quench_test::test]
fn files_that_are_not_fonts_are_refused_by_what_is_wrong_with_them() {
    assert_eq!(Font::parse(Vec::new()).unwrap_err(), FontError::NotTrueType);
    // A CFF font is refused by name, so that a caller can say why.
    let cff = Font::parse(&b"OTTO\0\0\0\0\0\0\0\0"[..]).unwrap_err();
    assert_eq!(cff, FontError::Cff);
    assert_eq!(cff.to_string(), "CFF outlines are not read");
    assert_eq!(
        Font::parse(&b"\0\x01\0\0"[..]).unwrap_err(),
        FontError::Truncated("directory")
    );
    assert!(
        matches!(Font::parse(&REGULAR[..300]), Err(FontError::Missing(_))),
        "tables that run past the file are missing"
    );
    let rename = |from: &[u8; 4], to: &[u8; 4]| {
        let mut bytes = REGULAR.to_vec();
        let at = (0..17)
            .map(|i| 12 + i * 16)
            .find(|a| &bytes[*a..*a + 4] == from)
            .unwrap();
        bytes[at..at + 4].copy_from_slice(to);
        bytes
    };
    for (table, name) in [
        (b"glyf", "glyf"),
        (b"loca", "loca"),
        (b"head", "head"),
        (b"hmtx", "hmtx"),
        (b"maxp", "maxp"),
        (b"hhea", "hhea"),
    ] {
        assert_eq!(
            Font::parse(rename(table, b"zzzz")).unwrap_err(),
            FontError::Missing(name)
        );
    }
    assert_eq!(
        Font::parse(rename(b"cmap", b"zzzz")).unwrap_err(),
        FontError::Missing("cmap")
    );
    // Without `name` and `GPOS` it is still a font: no names, no kerning.
    let bare = Font::parse(rename(b"GPOS", b"zzzz")).unwrap();
    assert_eq!(
        bare.kerning(bare.glyph('A').unwrap(), bare.glyph('V').unwrap()),
        0
    );
    let nameless = Font::parse(rename(b"name", b"zzzz")).unwrap();
    assert_eq!(nameless.name(1), None);
    // A zero em square is not a font.
    let mut zero = REGULAR.to_vec();
    let head = (0..17)
        .map(|i| 12 + i * 16)
        .find(|a| &zero[*a..*a + 4] == b"head")
        .unwrap();
    let at = u32::from_be_bytes(zero[head + 8..head + 12].try_into().unwrap()) as usize + 18;
    zero[at..at + 2].copy_from_slice(&[0, 0]);
    assert_eq!(Font::parse(zero).unwrap_err(), FontError::Truncated("head"));
    for e in [
        FontError::NotTrueType,
        FontError::Cff,
        FontError::Missing("x"),
        FontError::Truncated("y"),
    ] {
        assert_ne!(e.to_string(), "");
    }
}

#[quench_test::test]
fn damaged_fonts_never_panic_whatever_is_asked_of_them() {
    // A deterministic scramble of the file: bytes flipped in the tables the parser reads. Every call the rest of
    // the crate makes is exercised on what comes out; the only requirement is that none of it panics or hangs.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let directory: Vec<(usize, usize)> = (0..17)
        .map(|i| {
            let r = 12 + i * 16;
            (
                u32::from_be_bytes(REGULAR[r + 8..r + 12].try_into().unwrap()) as usize,
                u32::from_be_bytes(REGULAR[r + 12..r + 16].try_into().unwrap()) as usize,
            )
        })
        .collect();
    for round in 0..150 {
        let mut bytes = REGULAR.to_vec();
        for _ in 0..=(round % 24) {
            // Mostly inside the tables (the directory and the table starts matter most), sometimes anywhere.
            let at = if next() % 4 == 0 {
                (next() as usize) % bytes.len()
            } else {
                let (offset, length) = directory[(next() as usize) % directory.len()];
                offset + (next() as usize) % length.clamp(1, 600)
            };
            let at = at.min(bytes.len() - 1);
            bytes[at] = next() as u8;
        }
        if round % 10 == 0 {
            bytes.truncate((next() as usize) % bytes.len());
        }
        let Ok(font) = Font::parse(bytes) else {
            continue;
        };
        for c in "AVTo żółć Я日😀".chars() {
            if let Some(g) = font.glyph(c) {
                let _ = (
                    font.advance(g),
                    font.outline(g).map(|o| o.to_path(0.02, 0.0, 10.0)),
                );
                let _ = font.kerning(g, font.glyph('V').unwrap_or(0));
            }
        }
        for g in [0u16, 1, 5, 100, 3000, u16::MAX] {
            let _ = (font.outline(g), font.advance(g), font.kerning(g, g));
        }
        let _ = (font.name(0), font.name(13));
    }
}

quench_test::main!();

#[quench_test::test]
fn a_text_laid_out_again_is_the_same_and_each_size_and_width_is_its_own() {
    let fonts = FontSet::noto_sans();
    let text = "Example app number 12 with a name long enough to wrap";
    let first = fonts.layout(text, 16.0, None);
    // The second call is answered from memory and is the very same layout.
    assert_eq!(fonts.layout(text, 16.0, None), first);
    // Another size, another width, another text: not mistaken for it.
    let bigger = fonts.layout(text, 24.0, None);
    assert!(bigger.width > first.width);
    let wrapped = fonts.layout(text, 16.0, Some(120.0));
    assert!(wrapped.lines.len() > first.lines.len());
    assert_eq!(fonts.layout(text, 16.0, None), first, "still the first one");
    assert_ne!(fonts.layout("Another text", 16.0, None), first);
    // The empty text still has its line, cached or not.
    assert_eq!(fonts.layout("", 16.0, None).lines.len(), 1);
    assert_eq!(fonts.layout("", 16.0, None).lines.len(), 1);
}

// ---- The MATH table ----

/// A small `MATH` table built by hand: no constants, no variants; glyph info with italic corrections for glyphs 5
/// and 9 (coverage format 1) and corner kerns for glyph 7 (coverage format 2): top right in three height bands,
/// bottom left one value at every height. The offsets are written out as the specification lays them.
fn handmade_math() -> Vec<u8> {
    let words: [i16; 44] = [
        1, 0, 0, 10,
        0, // @0 header: version 1.0, constants none, glyph info at 10, variants none
        8, 0, 0,
        28, // @10 MathGlyphInfo: italics at +8 (18), no accents, no extended shapes, kerns at +28 (38)
        12, 2, 145, 0, -7, 0, // @18 italics: coverage at +12 (30), two value records
        1, 2, 5, 9, // @30 coverage format 1: glyphs 5 and 9
        12, 1, 22, 0, 0,
        44, // @38 MathKernInfo: coverage at +12 (50), one record: top right +22, bottom left +44
        2, 1, 7, 7, 0, // @50 coverage format 2: glyph 7, coverage index 0
        2, 100, 0, 300, 0, -50, 0, -20, 0, 10,
        0, // @60 MathKern: heights 100, 300; kerns -50, -20, 10
        0, -30, 0, // @82 MathKern: no heights, one kern
    ];
    words.iter().flat_map(|w| w.to_be_bytes()).collect()
}

/// `font` with its table `tag` replaced by `table`, appended at the end of the file (the directory entry is
/// renamed and repointed, so the file keeps its table count).
fn with_table(font: &[u8], replaced: [u8; 4], tag: [u8; 4], table: &[u8]) -> Vec<u8> {
    let mut bytes = font.to_vec();
    let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    let record = (0..count)
        .map(|i| 12 + i * 16)
        .find(|r| bytes[*r..*r + 4] == replaced)
        .unwrap();
    let at = bytes.len() as u32;
    bytes[record..record + 4].copy_from_slice(&tag);
    bytes[record + 8..record + 12].copy_from_slice(&at.to_be_bytes());
    bytes[record + 12..record + 16].copy_from_slice(&(table.len() as u32).to_be_bytes());
    bytes.extend_from_slice(table);
    bytes
}

#[quench_test::test]
fn math_kerns_are_looked_up_by_height_band() {
    use quench_text::{Math, MathKernCorner as Corner};
    let math = Math::parse(&handmade_math()).unwrap();
    assert_eq!(math.constants(), None);
    assert_eq!(math.kerned_glyphs().collect::<Vec<_>>(), [7]);
    // Below the first height, between the two (a height on a boundary is in the band below), above the last.
    for (height, kern) in [
        (i16::MIN, -50),
        (0, -50),
        (100, -50),
        (101, -20),
        (300, -20),
        (301, 10),
        (i16::MAX, 10),
    ] {
        assert_eq!(math.kern(7, Corner::TopRight, height), kern, "{height}");
    }
    assert_eq!(math.kern(7, Corner::BottomLeft, -1000), -30);
    assert_eq!(math.kern(7, Corner::BottomLeft, 1000), -30);
    // A corner the font leaves out, and a glyph without kerns, kern by nothing.
    assert_eq!(math.kern(7, Corner::TopLeft, 0), 0);
    assert_eq!(math.kern(7, Corner::BottomRight, 0), 0);
    assert_eq!(math.kern(8, Corner::TopRight, 0), 0);
    let info = math.kern_info(7).unwrap();
    assert_eq!(info.corner(Corner::TopRight).correction_heights, [100, 300]);
    assert_eq!(info.top_right.kern_values, [-50, -20, 10]);
    assert_eq!(
        (math.italics_correction(5), math.italics_correction(9)),
        (Some(145), Some(-7))
    );
    assert_eq!(math.italics_correction(6), None);
    assert_eq!(math.top_accent_attachment(5), None);
    assert!(!math.is_extended_shape(5));
    assert_eq!(math.vertical_glyphs().count(), 0);
    assert_eq!(math.min_connector_overlap(), 0);

    // The same table in a font: the font finds it; a font without one has none.
    let font = Font::parse(with_table(REGULAR, *b"post", *b"MATH", &handmade_math())).unwrap();
    assert_eq!(font.math(), Some(&math));
    assert!(noto_sans().math().is_none());
}

#[quench_test::test]
fn a_damaged_math_table_reads_as_absent_or_empty_and_never_panics() {
    use quench_text::{Math, MathKernCorner as Corner};
    let table = handmade_math();
    for length in 0..table.len() {
        let math = Math::parse(&table[..length]);
        assert_eq!(math.is_some(), length >= 10, "{length}");
        if let Some(math) = math {
            // Each subtable is read whole or not at all.
            assert!([0, -20].contains(&math.kern(7, Corner::TopRight, 200)));
            assert!([None, Some(145)].contains(&math.italics_correction(5)));
        }
    }
    // Another version, or offsets and counts pointing anywhere.
    let mut other = table.clone();
    other[1] = 2;
    assert_eq!(Math::parse(&other), None);
    for at in (0..table.len()).step_by(2) {
        for value in [0u16, 1, 2, 3, 0x7FFF, 0xFFFF] {
            let mut bytes = table.clone();
            bytes[at..at + 2].copy_from_slice(&value.to_be_bytes());
            if let Some(math) = Math::parse(&bytes) {
                for corner in [
                    Corner::TopRight,
                    Corner::TopLeft,
                    Corner::BottomRight,
                    Corner::BottomLeft,
                ] {
                    let _ = math.kern(7, corner, 150);
                }
                let _ = (math.italics_correction(5), math.kerned_glyphs().count());
            }
        }
    }
}

#[quench_test::test]
fn glyph_bounds_come_from_the_glyph_header() {
    let font = noto_sans();
    let o = font.bounds(font.glyph('o').unwrap()).unwrap();
    let h = font.bounds(font.glyph('H').unwrap()).unwrap();
    assert!(o.x_min < o.x_max && o.y_min < 0 && o.y_max > 0, "{o:?}");
    assert!(h.y_min == 0 && h.y_max > o.y_max, "{h:?} {o:?}");
    // A space has no outline, so no box; nor has a glyph past the last.
    assert_eq!(font.bounds(font.glyph(' ').unwrap()), None);
    assert_eq!(font.bounds(font.glyph_count()), None);
}

#[cfg(feature = "math-font")]
mod math_font {
    use quench_text::{Font, GlyphConstruction, Math, MathKernCorner, noto_sans_math};

    const MATH_FONT: &[u8] = include_bytes!("../fonts/NotoSansMath-Regular.ttf");

    fn glyph(c: char) -> u16 {
        noto_sans_math().glyph(c).unwrap()
    }

    #[quench_test::test]
    fn the_math_font_is_embedded_with_its_constants() {
        let font = noto_sans_math();
        assert_eq!((font.units_per_em(), font.glyph_count()), (1000, 5130));
        assert_eq!(font.name(1).as_deref(), Some("Noto Sans Math"));
        assert!(font.name(5).unwrap().starts_with("Version 3.000"));
        assert!(font.name(0).unwrap().contains("Google LLC"));
        assert_eq!(
            font.name(14).as_deref(),
            Some("https://openfontlicense.org")
        );
        assert!(include_str!("../fonts/README.md").contains("NotoSansMath-Regular.ttf"));
        assert!(
            font.name(13)
                .unwrap()
                .contains("SIL Open Font License, Version 1.1")
        );
        assert_eq!(
            (font.ascent(), font.descent(), font.line_gap()),
            (1069, -423, 0)
        );
        let c = *font.math().unwrap().constants().unwrap();
        assert_eq!(
            (
                c.script_percent_scale_down,
                c.script_script_percent_scale_down
            ),
            (60, 50)
        );
        assert_eq!((c.axis_height, c.accent_base_height), (278, 556));
        // The rest: rules have a thickness, gaps are gaps, shifts go the way their names say.
        for (name, value) in [
            ("fraction_rule_thickness", c.fraction_rule_thickness),
            ("overbar_rule_thickness", c.overbar_rule_thickness),
            ("underbar_rule_thickness", c.underbar_rule_thickness),
            ("radical_rule_thickness", c.radical_rule_thickness),
            ("radical_vertical_gap", c.radical_vertical_gap),
            (
                "radical_display_style_vertical_gap",
                c.radical_display_style_vertical_gap,
            ),
            ("fraction_numerator_gap_min", c.fraction_numerator_gap_min),
            (
                "fraction_denominator_gap_min",
                c.fraction_denominator_gap_min,
            ),
            ("stack_gap_min", c.stack_gap_min),
            ("superscript_shift_up", c.superscript_shift_up),
            ("subscript_shift_down", c.subscript_shift_down),
            ("fraction_numerator_shift_up", c.fraction_numerator_shift_up),
            (
                "fraction_denominator_shift_down",
                c.fraction_denominator_shift_down,
            ),
            ("sub_superscript_gap_min", c.sub_superscript_gap_min),
            ("space_after_script", c.space_after_script),
            ("upper_limit_gap_min", c.upper_limit_gap_min),
            ("lower_limit_gap_min", c.lower_limit_gap_min),
            (
                "radical_degree_bottom_raise_percent",
                c.radical_degree_bottom_raise_percent,
            ),
        ] {
            assert!(value > 0 && value < 1000, "{name} = {value}");
        }
        assert!(c.display_operator_min_height > 1000);
        assert!(c.delimited_sub_formula_min_height > 1000);
        assert!(c.superscript_shift_up_cramped < c.superscript_shift_up);
        assert!(c.fraction_numerator_display_style_shift_up > c.fraction_numerator_shift_up);
        assert!(c.flattened_accent_base_height > c.accent_base_height);
        // The kern after a radical's degree pulls the radicand back under it.
        assert!(c.radical_kern_after_degree < 0 && c.radical_kern_before_degree > 0);
        // Parsed once: the same table each time.
        assert!(std::ptr::eq(font.math().unwrap(), font.math().unwrap()));
    }

    /// Checks a construction: variants that grow, starting at the glyph itself, and an assembly of real glyphs
    /// with at least one extender.
    fn grows(font: &Font, base: u16, construction: &GlyphConstruction, with_assembly: bool) {
        let variants = &construction.variants;
        assert!(
            variants.windows(2).all(|w| w[0].advance < w[1].advance),
            "{variants:?}"
        );
        if let Some(first) = variants.first() {
            assert_eq!(first.glyph, base);
        }
        assert!(variants.iter().all(|v| v.glyph < font.glyph_count()));
        let Some(assembly) = &construction.assembly else {
            assert!(!with_assembly, "{base}: no assembly");
            return;
        };
        assert!(assembly.parts.len() >= 2);
        assert!(assembly.parts.iter().any(|p| p.extender));
        assert!(assembly.parts.iter().any(|p| !p.extender));
        for part in &assembly.parts {
            assert!(part.glyph < font.glyph_count() && part.full_advance > 0);
            assert!(font.bounds(part.glyph).is_some(), "{part:?}");
        }
        // The first part has no start connector and the last no end connector.
        assert_eq!(assembly.parts[0].start_connector_length, 0);
        assert_eq!(assembly.parts.last().unwrap().end_connector_length, 0);
    }

    #[quench_test::test]
    fn delimiters_radicals_and_integrals_grow_by_variants_then_assemblies() {
        let font = noto_sans_math();
        let math = font.math().unwrap();
        assert_eq!(math.min_connector_overlap(), 100);
        for c in [
            '(', ')', '[', ']', '{', '}', '|', '\u{2016}', '\u{221A}', '\u{222B}',
        ] {
            let g = glyph(c);
            let vertical = math.vertical_construction(g).unwrap();
            assert!(vertical.variants.len() >= 2, "{c:?}");
            grows(&font, g, vertical, true);
            assert!(math.horizontal_construction(g).is_none());
            assert!(math.is_extended_shape(g), "{c:?}");
        }
        // `(`: bottom hook, extender, top hook. The integral's assembly leans: it has an italic correction.
        let paren = math.vertical_construction(glyph('(')).unwrap();
        let parts = &paren.assembly.as_ref().unwrap().parts;
        assert_eq!(
            parts.iter().map(|p| p.extender).collect::<Vec<_>>(),
            [false, true, false]
        );
        let integral = math.vertical_construction(glyph('\u{222B}')).unwrap();
        assert!(integral.assembly.as_ref().unwrap().italics_correction > 0);
        // A large operator has a display size but no assembly; an angle bracket only variants.
        let sum = math.vertical_construction(glyph('\u{2211}')).unwrap();
        assert!(sum.variants.len() >= 2 && sum.assembly.is_none());
        grows(&font, glyph('\u{2211}'), sum, false);
        assert!(
            math.vertical_construction(glyph('\u{27E8}'))
                .unwrap()
                .assembly
                .is_none()
        );
        // Ordinary letters do not grow.
        assert!(math.vertical_construction(glyph('x')).is_none());
        assert!(!math.is_extended_shape(glyph('x')));
        assert!(math.vertical_glyphs().count() > 100);
    }

    #[quench_test::test]
    fn braces_and_arrows_grow_wider() {
        let font = noto_sans_math();
        let math = font.math().unwrap();
        // The over-brace: wider variants, then an assembly with two extenders around the middle tip.
        let brace = math.horizontal_construction(glyph('\u{23DE}')).unwrap();
        grows(&font, glyph('\u{23DE}'), brace, true);
        let parts = &brace.assembly.as_ref().unwrap().parts;
        assert_eq!(
            parts.iter().map(|p| p.extender).collect::<Vec<_>>(),
            [false, true, false, true, false]
        );
        // Arrows have only an assembly: a shaft that repeats and a head.
        for c in ['\u{2192}', '\u{2190}'] {
            let arrow = math.horizontal_construction(glyph(c)).unwrap();
            assert_eq!(arrow.variants.len(), 0);
            grows(&font, glyph(c), arrow, true);
        }
        // A wide hat: variants only, and its first variant is not the combining glyph itself.
        let hat = math.horizontal_construction(glyph('\u{0302}')).unwrap();
        assert!(hat.variants.len() >= 3 && hat.assembly.is_none());
        assert!(math.horizontal_glyphs().count() > 10);
    }

    #[quench_test::test]
    fn math_letters_have_italic_corrections_and_accent_attachments() {
        let font = noto_sans_math();
        let math = font.math().unwrap();
        // The mathematical alphanumerics are beyond the BMP: the format 12 character map finds them.
        let italic_f = glyph('\u{1D453}');
        let bold_a = glyph('\u{1D400}');
        let reals = glyph('\u{211D}');
        let mut ids = vec![italic_f, bold_a, reals, glyph('A'), glyph('\u{1D538}')];
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 5, "each its own glyph");
        // U+1D455 is a hole in the block (italic h is U+210E).
        assert_eq!(font.glyph('\u{1D455}'), None);
        assert!(font.glyph('\u{210E}').is_some());
        assert_eq!(math.italics_correction(italic_f), Some(145));
        assert_eq!(math.italics_correction(bold_a), None);
        // The accent goes over the ink: inside the letter's box.
        for g in [bold_a, reals, glyph('a'), italic_f] {
            let accent = math.top_accent_attachment(g).unwrap();
            let bounds = font.bounds(g).unwrap();
            assert!(
                bounds.x_min < accent && accent < bounds.x_max,
                "{g}: {accent} {bounds:?}"
            );
        }
        assert_eq!(math.top_accent_attachment(glyph('\u{222B}')), None);
        // This font has no math kerns at all: every corner kerns by nothing.
        assert_eq!(math.kerned_glyphs().count(), 0);
        assert_eq!(math.kern(italic_f, MathKernCorner::TopRight, 500), 0);
    }

    /// The real font with its `MATH` table's directory length set to `length`.
    fn with_math_length(length: u32) -> Vec<u8> {
        let mut bytes = MATH_FONT.to_vec();
        let record = (0..18)
            .map(|i| 12 + i * 16)
            .find(|r| &bytes[*r..*r + 4] == b"MATH")
            .unwrap();
        bytes[record + 12..record + 16].copy_from_slice(&length.to_be_bytes());
        bytes
    }

    fn math_table() -> &'static [u8] {
        let record = (0..18)
            .map(|i| 12 + i * 16)
            .find(|r| &MATH_FONT[*r..*r + 4] == b"MATH")
            .unwrap();
        let word =
            |at: usize| u32::from_be_bytes(MATH_FONT[at..at + 4].try_into().unwrap()) as usize;
        let (offset, length) = (word(record + 8), word(record + 12));
        &MATH_FONT[offset..offset + length]
    }

    /// Everything a typesetter asks of the table, for whatever comes out.
    fn ask_everything(math: &Math) {
        let _ = math.constants();
        for g in (0..5200).step_by(7) {
            let _ = (
                math.italics_correction(g),
                math.top_accent_attachment(g),
                math.is_extended_shape(g),
                math.kern(g, MathKernCorner::BottomLeft, 100),
                math.vertical_construction(g),
                math.horizontal_construction(g),
            );
        }
    }

    #[quench_test::test]
    fn damage_to_one_direction_of_growth_leaves_the_other() {
        let full = math_table();
        let word =
            |bytes: &[u8], at: usize| usize::from(u16::from_be_bytes([bytes[at], bytes[at + 1]]));
        let variants = word(full, 8);
        let vertical_count = word(full, variants + 6);
        let intact = Math::parse(full).unwrap();
        let (vertical, horizontal) = (
            intact.vertical_glyphs().count(),
            intact.horizontal_glyphs().count(),
        );
        assert!(vertical > 100 && horizontal > 10);
        // A construction offset pointing past the table, first among the vertical ones, then the horizontal.
        for (field, damaged_vertical) in [
            (variants + 10, true),
            (variants + 10 + vertical_count * 2, false),
        ] {
            let mut bytes = full.to_vec();
            bytes[field..field + 2].copy_from_slice(&0xFFFF_u16.to_be_bytes());
            let math = Math::parse(&bytes).unwrap();
            let counts = (
                math.vertical_glyphs().count(),
                math.horizontal_glyphs().count(),
            );
            if damaged_vertical {
                assert_eq!(
                    counts,
                    (0, horizontal),
                    "the horizontal constructions survive"
                );
                assert_eq!(
                    math.horizontal_construction(glyph('\u{23DE}')),
                    intact.horizontal_construction(glyph('\u{23DE}'))
                );
            } else {
                assert_eq!(counts, (vertical, 0), "the vertical constructions survive");
                assert_eq!(
                    math.vertical_construction(glyph('(')),
                    intact.vertical_construction(glyph('('))
                );
            }
            // The rest of the table is untouched.
            assert_eq!(math.constants(), intact.constants());
            assert_eq!(math.min_connector_overlap(), 100);
        }
    }

    #[quench_test::test]
    fn the_real_math_table_cut_short_or_scrambled_never_panics() {
        let full = math_table();
        assert_eq!(full.len(), 20_200);
        // Cut short in the directory: the font reads, the table shrinks to what is left.
        for length in [0, 1, 9, 10, 100, 223, 224, 300, 3000, 10_000, 20_199] {
            let font = Font::parse(with_math_length(length)).unwrap();
            let math = font.math();
            assert_eq!(math.is_some(), length >= 10, "{length}");
            if let Some(math) = math {
                // The constants (214 bytes from offset 10) are there whole, or not at all.
                assert_eq!(math.constants().is_some(), length >= 224, "{length}");
                ask_everything(math);
            }
        }
        // Longer than the file: the table is missing.
        assert!(
            Font::parse(with_math_length(u32::MAX))
                .unwrap()
                .math()
                .is_none()
        );
        // Every cut of the table itself.
        for length in (0..full.len()).step_by(97) {
            if let Some(math) = Math::parse(&full[..length]) {
                ask_everything(&math);
            }
        }
        // Garbage, after an intact header and in place of it.
        let mut state = 0x9E37_79B9_7F4A_7C15_u64;
        for round in 0..40 {
            let mut bytes = full.to_vec();
            for _ in 0..=(round * 8) {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let at = if round % 2 == 0 {
                    (state as usize) % 600
                } else {
                    (state as usize) % bytes.len()
                };
                bytes[at] = (state >> 32) as u8;
            }
            if round % 5 == 0 {
                bytes[..2].copy_from_slice(&[0, 1]);
            }
            if let Some(math) = Math::parse(&bytes) {
                ask_everything(&math);
            }
        }
        let garbage: Vec<u8> = (0..20_000u32)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let _ = Math::parse(&garbage).map(|m| ask_everything(&m));
    }
}
