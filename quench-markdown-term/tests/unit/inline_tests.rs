use crate::plain;
use quench_markdown_term::{
    AnsiOptions, LinkKind, Options, Role, render_markdown, to_ansi, to_plain,
};

fn no_numbers(width: usize) -> Options {
    Options {
        width,
        numbered_links: false,
        ..Options::default()
    }
}

#[quench_test::test]
fn styles_are_kept_without_the_markup() {
    let rendered = render_markdown("*a* **b** ~~c~~ `d`", &Options::default());
    let spans: Vec<(&str, bool, bool, bool, bool)> = rendered.lines[0]
        .spans
        .iter()
        .filter(|s| s.text.trim() != "")
        .map(|s| {
            (
                s.text.as_str(),
                s.style.italic,
                s.style.bold,
                s.style.strikethrough,
                s.style.code,
            )
        })
        .collect();
    assert_eq!(
        spans,
        [
            ("a", true, false, false, false),
            ("b", false, true, false, false),
            ("c", false, false, true, false),
            ("d", false, false, false, true)
        ]
    );
    assert_eq!(rendered.lines[0].text(), "a b c d");
}

#[quench_test::test]
fn links_show_their_text_and_number_their_destination() {
    let text = "See [the docs](https://example.com/docs \"Docs\"), [again](https://example.com/docs), \
                <https://auto.example/> and [a section](#usage).";
    assert_eq!(
        plain(text, 40),
        "See the docs [1], again [1],\nhttps://auto.example/ and a section.\n\n\
         ────────────────────────────────────────\n[1] https://example.com/docs\n"
    );
    let rendered = render_markdown(text, &Options::default());
    let link = &rendered.links[0];
    assert_eq!(
        (
            link.destination.as_str(),
            link.title.as_str(),
            link.kind,
            link.number
        ),
        ("https://example.com/docs", "Docs", LinkKind::Link, Some(1))
    );
    assert_eq!(rendered.links[2].number, None);
    let span = rendered.lines[0]
        .spans
        .iter()
        .find(|s| s.text == "the docs")
        .unwrap();
    assert_eq!(
        (span.style.role, span.style.underline, span.style.link),
        (Role::Link, true, Some(0))
    );
}

#[quench_test::test]
fn hyperlinks_on_write_osc_8_around_the_text() {
    let rendered = render_markdown("Go [there](https://example.com) now.", &no_numbers(40));
    let options = AnsiOptions {
        hyperlinks: true,
        colour: false,
        ..AnsiOptions::default()
    };
    assert_eq!(
        to_ansi(&rendered, &options),
        "Go \x1b]8;;https://example.com\x1b\\\x1b[4mthere\x1b]8;;\x1b\\\x1b[0m now.\n"
    );
    assert_eq!(to_plain(&rendered), "Go there now.\n");
}

#[quench_test::test]
fn hyperlinks_off_write_no_osc_8() {
    let rendered = render_markdown(
        "Go [there](https://example.com) now.",
        &Options {
            width: 40,
            ..Options::default()
        },
    );
    let ansi = to_ansi(
        &rendered,
        &AnsiOptions {
            colour: false,
            ..AnsiOptions::default()
        },
    );
    assert!(!ansi.contains("\x1b]8"));
    assert!(
        ansi.starts_with("Go \x1b[4mthere\x1b[0m\x1b[2m [1]\x1b[0m now.\n"),
        "{ansi:?}"
    );
    assert!(
        ansi.ends_with("\x1b[2m[1] https://example.com\x1b[0m\n"),
        "{ansi:?}"
    );
}

#[quench_test::test]
fn relative_links_are_not_hyperlinks() {
    let rendered = render_markdown("[page](../other.md) and [anchor](#top)", &no_numbers(40));
    let ansi = to_ansi(
        &rendered,
        &AnsiOptions {
            hyperlinks: true,
            ..AnsiOptions::default()
        },
    );
    assert!(!ansi.contains("\x1b]8"));
    assert_eq!(rendered.links[0].destination, "../other.md");
}

#[quench_test::test]
fn ansi_colours_by_role_and_resets_at_line_ends() {
    let rendered = render_markdown(
        "# Title\n\n`code`",
        &Options {
            width: 10,
            ..Options::default()
        },
    );
    assert_eq!(
        to_ansi(&rendered, &AnsiOptions::default()),
        "\x1b[1;35mTitle\x1b[0m\n\x1b[35m══════════\x1b[0m\n\n\x1b[32mcode\x1b[0m\n"
    );
    let no_colour = AnsiOptions {
        colour: false,
        ..AnsiOptions::default()
    };
    assert_eq!(
        to_ansi(&rendered, &no_colour),
        "\x1b[1mTitle\x1b[0m\n══════════\n\ncode\n"
    );
}

#[quench_test::test]
fn images_show_their_alt_text() {
    assert_eq!(
        plain("![a diagram](img/d.png) ![](x.png)", 40),
        "[image: a diagram] [1] [image] [2]\n\n────────────────────────────────────────\n[1] img/d.png\n[2] x.png\n"
    );
    let rendered = render_markdown("![a diagram](img/d.png)", &no_numbers(40));
    assert_eq!(rendered.links[0].kind, LinkKind::Image);
    assert_eq!(rendered.lines[0].spans[0].style.role, Role::Image);
}

#[quench_test::test]
fn inline_html_is_dim_as_written() {
    let rendered = render_markdown("Press <kbd>q</kbd>.", &Options::default());
    assert_eq!(rendered.lines[0].text(), "Press <kbd>q</kbd>.");
    let html = rendered.lines[0]
        .spans
        .iter()
        .find(|s| s.text == "<kbd>")
        .unwrap();
    assert!(html.style.dim);
    assert_eq!(html.style.role, Role::Html);
}

#[quench_test::test]
fn control_characters_never_reach_the_output() {
    let out = plain(
        "evil \u{1b}[31mred\u{7} text\n\n```\n\u{1b}]8;;x\u{7}\n```\n",
        40,
    );
    assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
}

#[quench_test::test]
fn ascii_mode_uses_no_unicode_decoration() {
    let markdown = "# T\n\n- a\n  - b\n\n> q\n\n```\nx\n```\n\n| a |\n|---|\n| 1 |\n\n---\n\nn[^1]\n\n[^1]: d\n";
    let rendered = render_markdown(
        markdown,
        &Options {
            width: 30,
            unicode: false,
            ..Options::default()
        },
    );
    let text = to_plain(&rendered);
    assert!(text.is_ascii(), "{text}");
    assert!(text.contains("n[^1]"));
}
