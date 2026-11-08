use quench_markdown_term::{Options, Slugger, char_width, display_width, render_markdown, slug};

#[quench_test::test]
fn ascii_is_one_cell_per_character() {
    assert_eq!(display_width("hello, world"), 12);
    assert_eq!(display_width(""), 0);
}

#[quench_test::test]
fn combining_marks_take_no_cell() {
    assert_eq!(display_width("e\u{301}"), 1);
    assert_eq!(display_width("a\u{20D7}"), 1);
    assert_eq!(char_width('\u{200B}'), 0);
    assert_eq!(char_width('\u{1b}'), 0);
}

#[quench_test::test]
fn east_asian_wide_and_emoji_take_two() {
    assert_eq!(display_width("セン"), 4);
    assert_eq!(display_width("漢字"), 4);
    assert_eq!(display_width("한"), 2);
    assert_eq!(display_width("😀"), 2);
    assert_eq!(display_width("🚀 go"), 5);
    assert_eq!(display_width("ＡＢ"), 4);
    assert_eq!(display_width("…─│"), 3);
}

#[quench_test::test]
fn emoji_presentation_takes_two() {
    assert_eq!(display_width("\u{2764}"), 1);
    assert_eq!(display_width("\u{2764}\u{FE0F}"), 2);
    assert_eq!(display_width("#\u{FE0F}\u{20E3}"), 2);
    assert_eq!(display_width("\u{263A}\u{FE0E}"), 1);
    assert_eq!(display_width(" \u{FE0F}"), 1);
    assert_eq!(char_width('\u{FE0F}'), 0);
    assert_eq!(char_width('\u{FE0E}'), 0);
}

#[quench_test::test]
fn zwj_sequences_are_one_emoji() {
    assert_eq!(display_width("👨\u{200D}👩\u{200D}👧"), 2);
    assert_eq!(display_width("👨\u{200D}👩\u{200D}👧\u{200D}👦 x"), 4);
    assert_eq!(display_width("🏃\u{200D}\u{2640}\u{FE0F}"), 2);
    assert_eq!(display_width("🏳\u{FE0F}\u{200D}🌈"), 2);
    // A joiner outside an emoji sequence joins nothing.
    assert_eq!(display_width("a\u{200D}b"), 2);
    assert_eq!(display_width("😀\u{200D} x"), 4);
}

#[quench_test::test]
fn skin_tones_take_no_cell() {
    assert_eq!(display_width("👍🏽"), 2);
    assert_eq!(display_width("👩🏾\u{200D}💻"), 2);
    assert_eq!(char_width('\u{1F3FB}'), 0);
    assert_eq!(char_width('\u{1F3FF}'), 0);
}

#[quench_test::test]
fn flags_are_two_cells_a_pair() {
    assert_eq!(display_width("🇵🇱"), 2);
    assert_eq!(display_width("🇵🇱🇩🇪"), 4);
    assert_eq!(display_width("🇵"), 1);
    assert_eq!(display_width("🇵🇱🇩"), 3);
}

#[quench_test::test]
fn emoji_sequences_wrap_whole_and_fit() {
    let family = "👨\u{200D}👩\u{200D}👧";
    for text in [
        format!("{family}{family}{family}{family}"),
        "❤\u{FE0F}❤\u{FE0F}❤\u{FE0F}❤\u{FE0F}".to_owned(),
        "🇵🇱🇵🇱🇵🇱🇵🇱".to_owned(),
        "*❤*\u{FE0F}*❤*\u{FE0F}*❤*\u{FE0F}".to_owned(),
    ] {
        for width in 4..9 {
            let rendered = render_markdown(
                &text,
                &Options {
                    width,
                    ..Options::default()
                },
            );
            crate::assert_fits(&rendered, width, &text);
        }
    }
    let rendered = render_markdown(
        &format!("{family}{family}{family}"),
        &Options {
            width: 4,
            ..Options::default()
        },
    );
    let lines: Vec<String> = rendered
        .lines
        .iter()
        .map(quench_markdown_term::Line::text)
        .collect();
    assert_eq!(lines, [format!("{family}{family}"), family.to_owned()]);
}

#[quench_test::test]
fn wide_text_wraps_by_cells() {
    let rendered = render_markdown(
        "セン セン セン",
        &Options {
            width: 9,
            ..Options::default()
        },
    );
    let lines: Vec<String> = rendered
        .lines
        .iter()
        .map(quench_markdown_term::Line::text)
        .collect();
    assert_eq!(lines, ["セン セン", "セン"]);
    let rendered = render_markdown(
        "センセンセン",
        &Options {
            width: 5,
            ..Options::default()
        },
    );
    let lines: Vec<String> = rendered
        .lines
        .iter()
        .map(quench_markdown_term::Line::text)
        .collect();
    assert_eq!(lines, ["セン", "セン", "セン"]);
}

#[quench_test::test]
fn slugs_follow_github() {
    assert_eq!(slug("Hello, World!"), "hello-world");
    assert_eq!(slug("What is missing"), "what-is-missing");
    assert_eq!(slug("`quench-html` and C++"), "quench-html-and-c");
    assert_eq!(slug("Ünïcode Straße"), "ünïcode-straße");
    assert_eq!(slug("snake_case and  two"), "snake_case-and--two");
    assert_eq!(slug("1. Numbers"), "1-numbers");
}

#[quench_test::test]
fn duplicate_slugs_are_numbered() {
    let mut slugger = Slugger::new();
    assert_eq!(slugger.slug("Usage"), "usage");
    assert_eq!(slugger.slug("Usage"), "usage-1");
    assert_eq!(slugger.slug("Usage"), "usage-2");
    assert_eq!(slugger.slug("Usage 1"), "usage-1-1");
}

#[quench_test::test]
fn headings_record_slugs_and_lines() {
    let rendered = render_markdown(
        "# Intro\n\ntext\n\n## Intro\n\n### Own anchor {#custom}\n",
        &Options {
            width: 40,
            ..Options::default()
        },
    );
    let spots: Vec<(u8, &str, &str, usize)> = rendered
        .headings
        .iter()
        .map(|h| (h.level, h.text.as_str(), h.slug.as_str(), h.line))
        .collect();
    assert_eq!(
        spots,
        [
            (1, "Intro", "intro", 0),
            (2, "Intro", "intro-1", 5),
            (3, "Own anchor", "custom", 8)
        ]
    );
    assert_eq!(rendered.lines[8].text(), "Own anchor");
    assert_eq!(rendered.heading_line("intro-1"), Some(5));
    assert_eq!(rendered.heading_line("missing"), None);
}
