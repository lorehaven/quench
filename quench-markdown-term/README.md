# quench-markdown-term
Markdown laid out for a terminal: the `quench-html` syntax tree as styled lines for a width, with ANSI and plain-text writers; no third-party dependencies (it builds on quench-html, and on quench-tex with the `tex` feature).

## What it is
`quench-markdown-term` is a second renderer beside `quench-html`'s HTML one. It takes a parsed `Document` (CommonMark with GFM tables, strikethrough, task lists, footnotes and math) and lays it out for a width in terminal cells: paragraphs wrapped at spaces, headings by level, code blocks in a frame (wrapped with a continuation mark, never reflowed), quotes and lists indented and nested, tables sized to their content (shrinking the widest columns, or stacked as `header: value` records when even that does not fit), footnotes at the end or after their paragraph. Every line fits the width; `display_width` counts East Asian wide characters and emoji as two cells and combining marks as none, and counts emoji sequences as one picture (`❤️` with U+FE0F, a ZWJ sequence such as `👨‍👩‍👧`, a flag: two cells each).

It is pure: no I/O and no terminal queries. The output keeps semantic roles (heading, link, code, quote, callout...) rather than colours; `to_ansi` colours them with a `Theme` of the 16 basic colours and writes OSC 8 hyperlinks, `to_plain` writes the text alone for pipes. Alongside the lines it records where headings (with GitHub-style slugs), links, footnotes and formulas ended up, for a pager to follow `#anchor` links, jump to a footnote, or put a typeset formula over its text.

Formulas go through a `MathText` hook; where it gives nothing (`SourceMath`, or `MathMode::Source`) the TeX source is shown, never a half-translated formula. What Jekyll leaves in a page is handled as the site shows it: kramdown attribute lists are dropped (a `.note` / `.warning` / `.tip`... class makes a callout), the `* TOC` / `{:toc}` placeholder disappears, `{% include x %}` shows as `[include: x]`, and `{{ site.baseurl }}` is replaced (but kept as written inside `{% raw %}`, as Jekyll does). `loupe` uses it to read Markdown and the documentation books in the terminal.

## Use cases
- Print a Markdown file to a terminal or a pipe, wrapped to its width.
- Feed a pager: lines plus the positions of headings, links, footnotes and formulas.
- Show Jekyll / just-the-docs pages with their callouts and links intact.
- Render Markdown answers in a command-line client.

## Example
```rust
use quench_markdown_term::{AnsiOptions, FootnotePlacement, Options, render_markdown, to_ansi, to_plain};

let page = "# Title\n\nSome *text*[^1] and [a link](https://example.com).\n\n[^1]: A note.\n";
let options = Options { width: 60, footnotes: FootnotePlacement::End, ..Options::default() };
let rendered = render_markdown(page, &options);
print!("{}", to_plain(&rendered));                     // links numbered `[1]`, listed at the end

let terminal = Options { numbered_links: false, ..options };
let rendered = render_markdown(page, &terminal);
let ansi = to_ansi(&rendered, &AnsiOptions { hyperlinks: true, ..AnsiOptions::default() });
print!("{ansi}");                                       // colours and OSC 8 hyperlinks
assert_eq!(rendered.headings[0].slug, "title");
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-markdown-term/
