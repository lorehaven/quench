use crate::snapshot;
use quench_markdown_term::{FootnotePlacement, Options};

const FOOTNOTES_END: &str = "Text with a note[^1] and another[^n], and the first again[^1].\n\nMore text.\n\n[^1]: The first note.\n[^n]: The second, longer note, which wraps at forty cells for sure.\n";

/// References as superscripts; definitions at the end under a rule, with back-marks.
#[quench_test::test]
fn footnotes_end() {
    snapshot(
        FOOTNOTES_END,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
Text with a note¹ and another², and the
first again¹.

More text.

────────────────────────────────────────
1. The first note. ↩
2. The second, longer note, which wraps
   at forty cells for sure. ↩
"#,
    );
    snapshot(
        FOOTNOTES_END,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
Text with a note¹ and another², and the first again¹.

More text.

────────────────────────────────────────────────────────────────────────────────
1. The first note. ↩
2. The second, longer note, which wraps at forty cells for sure. ↩
"#,
    );
    snapshot(
        FOOTNOTES_END,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
Text with a note¹ and another², and the first again¹.

More text.

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
1. The first note. ↩
2. The second, longer note, which wraps at forty cells for sure. ↩
"#,
    );
}

const FOOTNOTES_INLINE: &str = "Text with a note[^1] and another[^n].\n\nMore text.\n\n[^1]: The first note.\n[^n]: The second note.\n";

/// Each definition right after the block that first references it.
#[quench_test::test]
fn footnotes_inline() {
    snapshot(
        FOOTNOTES_INLINE,
        &Options {
            width: 40,
            footnotes: FootnotePlacement::Inline,
            ..Options::default()
        },
        r#"
Text with a note¹ and another².

  ¹ The first note.

  ² The second note.

More text.
"#,
    );
    snapshot(
        FOOTNOTES_INLINE,
        &Options {
            width: 80,
            footnotes: FootnotePlacement::Inline,
            ..Options::default()
        },
        r#"
Text with a note¹ and another².

  ¹ The first note.

  ² The second note.

More text.
"#,
    );
    snapshot(
        FOOTNOTES_INLINE,
        &Options {
            width: 120,
            footnotes: FootnotePlacement::Inline,
            ..Options::default()
        },
        r#"
Text with a note¹ and another².

  ¹ The first note.

  ² The second note.

More text.
"#,
    );
}

#[quench_test::test]
fn footnote_spots_and_links() {
    use quench_markdown_term::{LinkKind, render_markdown};
    let rendered = render_markdown(
        FOOTNOTES_END,
        &Options {
            width: 40,
            ..Options::default()
        },
    );
    let spots: Vec<(usize, &str, usize, Vec<usize>)> = rendered
        .footnotes
        .iter()
        .map(|f| {
            (
                f.number,
                f.label.as_str(),
                f.definition_line,
                f.reference_lines.clone(),
            )
        })
        .collect();
    assert_eq!(spots, [(1, "1", 6, vec![0, 1]), (2, "n", 7, vec![0])]);
    let reference = rendered.lines[0]
        .spans
        .iter()
        .find(|s| s.text == "¹")
        .unwrap();
    let target = &rendered.links[reference.style.link.unwrap()];
    assert_eq!(
        (target.kind, target.destination.as_str()),
        (LinkKind::Footnote(1), "#fn-1")
    );
    let back = rendered.lines[6]
        .spans
        .iter()
        .find(|s| s.text == "↩")
        .unwrap();
    let target = &rendered.links[back.style.link.unwrap()];
    assert_eq!(
        (target.kind, target.destination.as_str()),
        (LinkKind::FootnoteBack(1), "#fnref-1")
    );
}

#[quench_test::test]
fn inline_definitions_are_dim_and_recorded() {
    use quench_markdown_term::render_markdown;
    let options = Options {
        width: 40,
        footnotes: FootnotePlacement::Inline,
        ..Options::default()
    };
    let rendered = render_markdown(FOOTNOTES_INLINE, &options);
    assert_eq!(rendered.footnotes[0].definition_line, 2);
    assert_eq!(rendered.footnotes[1].definition_line, 4);
    assert!(rendered.lines[2].spans.iter().all(|s| s.style.dim));
}

#[quench_test::test]
fn a_footnote_in_a_list_item_follows_its_paragraph() {
    let options = Options {
        width: 40,
        footnotes: FootnotePlacement::Inline,
        ..Options::default()
    };
    snapshot(
        "- one[^a]\n- two\n\n[^a]: Note.\n",
        &options,
        r#"
• one¹
    ¹ Note.
• two
"#,
    );
}

/// Without Unicode a footnote mark is `[^1]`, so it is not taken for a link number ` [1]`.
#[quench_test::test]
fn ascii_footnote_marks_differ_from_link_numbers() {
    snapshot(
        "A [link](https://e.com) and a note[^a].\n\n[^a]: The note.\n",
        &Options {
            width: 40,
            unicode: false,
            ..Options::default()
        },
        r#"
A link [1] and a note[^1].

----------------------------------------
[^1] The note. ^

----------------------------------------
[1] https://e.com
"#,
    );
    snapshot(
        "A note[^a].\n\n[^a]: The note.\n",
        &Options {
            width: 40,
            unicode: false,
            footnotes: FootnotePlacement::Inline,
            ..Options::default()
        },
        r#"
A note[^1].

  [^1] The note.
"#,
    );
}
