// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::needless_raw_string_hashes
)]

use quench_markdown_term::{Options, Rendered, display_width, render_markdown, to_plain};

#[path = "unit/block_tests.rs"]
mod block_tests;
#[path = "unit/footnote_tests.rs"]
mod footnote_tests;
#[path = "unit/inline_tests.rs"]
mod inline_tests;
#[path = "unit/jekyll_tests.rs"]
mod jekyll_tests;
#[path = "unit/math_tests.rs"]
mod math_tests;
#[path = "unit/page_tests.rs"]
mod page_tests;
#[path = "unit/table_tests.rs"]
mod table_tests;
#[path = "unit/width_tests.rs"]
mod width_tests;

/// Asserts that no line of `rendered` is wider than `width` cells.
pub fn assert_fits(rendered: &Rendered, width: usize, what: &str) {
    for (i, line) in rendered.lines.iter().enumerate() {
        let cells = display_width(&line.text());
        assert!(
            cells <= width,
            "{what}: line {i} is {cells} cells wide, more than {width}: {:?}",
            line.text()
        );
    }
}

/// Renders `markdown` and compares the plain text with `expected` (written after a newline, for reading).
pub fn snapshot(markdown: &str, options: &Options, expected: &str) {
    let rendered = render_markdown(markdown, options);
    assert_fits(&rendered, options.width, "snapshot");
    let expected = expected.strip_prefix('\n').unwrap_or(expected);
    let got = to_plain(&rendered);
    assert!(
        got == expected,
        "at width {}:\n--- got\n{got}--- expected\n{expected}---",
        options.width
    );
}

/// `markdown` as plain text at `width`.
#[must_use]
pub fn plain(markdown: &str, width: usize) -> String {
    to_plain(&render_markdown(
        markdown,
        &Options {
            width,
            ..Options::default()
        },
    ))
}

quench_test::main!();
