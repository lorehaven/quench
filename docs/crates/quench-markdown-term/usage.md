---
title: Usage
parent: quench-markdown-term
grand_parent: Crates
nav_order: 1
---

Worked examples, from printing a page to driving a pager. The outputs shown are from the crate's tests (`tests/unit/*.rs`, `tests/tex.rs`) or were produced by running the code shown.

## 1. Print a Markdown page, to a terminal or a pipe

```rust
use quench_markdown_term::{AnsiOptions, FootnotePlacement, Options, render_markdown, to_ansi, to_plain};

let page = "# Title\n\nSome *text*[^1] and [a link](https://example.com).\n\n[^1]: A note.\n";

// A pipe: plain text, links numbered and listed at the end.
let options = Options { width: 60, footnotes: FootnotePlacement::End, ..Options::default() };
print!("{}", to_plain(&render_markdown(page, &options)));

// A terminal with OSC 8 hyperlinks: no numbers, colours and links in the escape codes.
let terminal = Options { numbered_links: false, ..options };
let rendered = render_markdown(page, &terminal);
print!("{}", to_ansi(&rendered, &AnsiOptions { hyperlinks: true, ..AnsiOptions::default() }));
assert_eq!(rendered.headings[0].slug, "title");
```

The plain output (60 cells):

```text
Title
════════════════════════════════════════════════════════════

Some text¹ and a link [1].

────────────────────────────────────────────────────────────
1. A note. ↩

────────────────────────────────────────────────────────────
[1] https://example.com
```

With `numbered_links: false`, the ` [1]` after the link and the link list at the end are not laid out. `to_ansi` then wraps "a link" in `ESC ] 8 ;; https://example.com ESC \` ... `ESC ] 8 ;; ESC \`, with SGR `4;34` (underline, blue).

Pitfalls:

* `numbered_links` is a layout option, not a writer option. Turn it off when you write with `hyperlinks: true`, or the reader sees both the hyperlink and the `[n]`.
* Get the width from the terminal yourself, for example `quench_cli::term::size()` in [quench-cli](../quench-cli/api-term.md). The crate does not query anything. Widths under 4 are laid out at 4.
* Strip YAML front matter before rendering. It is not recognised (see the [limitations](index.md#limitations)).

## 2. A tree parsed elsewhere

```rust
use quench_html::markdown::{self, Options as Syntax};
use quench_markdown_term::{Options, render, replace_baseurl, to_plain};

let source = replace_baseurl("[a]({{ site.baseurl }}/x)\n", "/book");
let document = markdown::parse(&source, Syntax::ALL);
let rendered = render(&document, &Options::default());
assert_eq!(rendered.links[0].destination, "/book/x");
let _ = to_plain(&rendered);
```

`render_markdown` replaces `{{ site.baseurl }}` in the source before parsing, the way Jekyll does. `[a]({{ site.baseurl }}/x)` is only a link once the spaces are gone. When you call `render` on your own `Document`, replace it first with `replace_baseurl`. `render` also replaces the placeholder in text and link destinations it meets, using `Options::baseurl`, but by then the parser has already decided that the bracket text is not a link. `replace_baseurl` handles `{{ site.baseurl }}`, `{{site.baseurl}}` and the two percent-encoded forms.

## 3. Feed a pager: headings, links and footnotes

```rust
use quench_markdown_term::{Options, render_markdown};

let rendered = render_markdown(
    "# Intro\n\ntext\n\n## Intro\n\n### Own anchor {#custom}\n",
    &Options { width: 40, ..Options::default() },
);
let spots: Vec<(u8, &str, &str, usize)> =
    rendered.headings.iter().map(|h| (h.level, h.text.as_str(), h.slug.as_str(), h.line)).collect();
assert_eq!(spots, [(1, "Intro", "intro", 0), (2, "Intro", "intro-1", 5), (3, "Own anchor", "custom", 8)]);
assert_eq!(rendered.heading_line("intro-1"), Some(5));   // follow a `#intro-1` link
```

* Every `Span` that belongs to a link has `style.link = Some(index)` into `rendered.links`. A `LinkTarget` has the destination, the title, the kind (`Link`, `Image`, `Footnote(n)`, `FootnoteBack(n)`) and the `[n]` number.
* `LinkTarget::is_url()` tells the links a terminal can open (a scheme such as `https:` or `mailto:`) from the ones the pager follows itself: relative paths and `#anchor`s.
* `rendered.footnotes[i]` gives `definition_line` and `reference_lines` for jumping back and forth. A footnote reference links to `#fn-<label>`, and the definition's `↩` links to `#fnref-<label>`.

Pitfall: line numbers are positions in `rendered.lines`. When you re-render at another width, look the spots up again.

## 4. A Jekyll / just-the-docs page

From `tests/unit/jekyll_tests.rs`, at 40 cells with `baseurl: "/book".to_owned()`:

```text
# Page
{: .no_toc }

* TOC
{:toc}

{% include nav.html %}

{% assign x = 1 %}

See [the index]({{ site.baseurl }}/index.html) and {{site.baseurl}}/raw.

`{% raw %}{% include x %}{% endraw %}`
```

renders as

```text
Page
════════════════════════════════════════

[include: nav.html]

See the index [1] and /book/raw.

{% include x %}

────────────────────────────────────────
[1] /book/index.html
```

These rules produce that output:

* The `{: .no_toc }` attribute list is dropped.
* The `* TOC` / `{:toc}` placeholder disappears.
* `{% include %}` shows as a dim `[include: name]`.
* Other Liquid tags (`assign`) are dropped.
* `{{ site.baseurl }}` is replaced.
* The inside of `{% raw %}` is kept literally, `{{ site.baseurl }}` included: `Kept {% raw %}{{ site.baseurl }}{% endraw %}` renders as `Kept {{ site.baseurl }}` (test `raw_keeps_baseurl`).

Pitfall: the renderer also replaces `{{ site.baseurl }}` in each text run, for pages parsed without `replace_baseurl`. It sees `{% raw %}` only within one run, so a raw block that spans markup (`{% raw %}*{{ site.baseurl }}*{% endraw %}`) still has its emphasised part replaced. Keep raw blocks inside plain text or code.

Callouts (same test file):

```text
{: .note }
A note callout with enough text to wrap at forty cells wide.

{: .important-title }
> Heads up
>
> Titled callout.
```

```text
┃ Note
┃ A note callout with enough text to
┃ wrap at forty cells wide.

┃ Heads up
┃ Titled callout.
```

The classes are `.note`, `.tip`, `.important`, `.warning`, `.highlight` and `.new`. With the `-title` suffix, the first paragraph of the quote becomes the label.

## 5. Formulas as text (feature `tex`)

```rust
use quench_html::markdown::{Options as Syntax, parse};
use quench_markdown_term::{Options, TexMath, render_with_math, to_plain};

let page = "Euler: $e^{i\\pi} + 1 = 0$.\n\n$$\n\\sum_{k=1}^{n} k = \\frac{n(n+1)}{2}\n$$\n";
let math = TexMath::new();                      // one per page: macros carry over
let rendered = render_with_math(&parse(page, Syntax::ALL), &Options { width: 40, ..Options::default() }, &math);
print!("{}", to_plain(&rendered));
```

```text
Euler: e^(iπ) + 1 = 0.

             n      n(n + 1)
             ∑  k = ────────
            k=1        2
```

Inline formulas become one line of Unicode (`$\pi r^2$` gives `πr²`). Display formulas are laid out in two dimensions by quench-tex and centred.

When quench-tex cannot read a formula, for example `\boxed{x}` or a syntax error, it gives `None`, and the TeX source is shown instead: a dim `$\boxed{x}$` inline, or a dim frame labelled `tex` for display math. A display formula wider than the line also falls back to its source.

Macros defined by `\newcommand` / `\def` stay for later formulas on the same `TexMath`:

```rust
use quench_markdown_term::{MathText, TexMath};

let math = TexMath::new();
assert_eq!(math.inline("\\newcommand{\\R}{\\mathbb{R}} x \\in \\R").unwrap(), "x ∈ ℝ");
assert_eq!(math.inline("\\R^2").unwrap(), "ℝ²");
```

Pitfall: `render` and `render_markdown` use `SourceMath`, so every formula shows as source. Use `render_with_math` to get text. `MathMode::Source` in `Options` forces source even with a hook.

## 6. Your own math hook

```rust
use quench_html::markdown::{Options as Syntax, parse};
use quench_markdown_term::{MathText, Options, render_with_math, to_plain};

/// Knows `x^2` and `\frac{a}{b}`, nothing else.
struct Known;

impl MathText for Known {
    fn inline(&self, tex: &str) -> Option<String> {
        (tex == "x^2").then(|| "x²".to_owned())
    }
    fn display(&self, tex: &str) -> Option<Vec<String>> {
        (tex.trim() == r"\frac{a}{b}").then(|| vec!["a".to_owned(), "─".to_owned(), "b".to_owned()])
    }
}

let page = "Inline $x^2$ and $y_1$ here.\n\n$$\n\\frac{a}{b}\n$$\n\n$$\n\\unknown{z}\n$$\n";
let rendered = render_with_math(&parse(page, Syntax::ALL), &Options { width: 20, ..Options::default() }, &Known);
assert_eq!(
    to_plain(&rendered),
    "Inline x² and $y_1$\nhere.\n\n         a\n         ─\n         b\n\n\
     ╭─ tex ────────────╮\n│ \\unknown{z}      │\n╰──────────────────╯\n"
);
```

`rendered.formulas` records where each formula ended up: (line, column, width, height, tex, display). Here those are `(0, 7, 2, 1, "x^2", false)`, `(0, 14, 5, 1, "y_1", false)`, `(3, 9, 1, 3, "\frac{a}{b}", true)` and `(7, 0, 20, 3, "\unknown{z}", true)`.

## 7. Typeset images over the text

A pager that can draw images uses the `FormulaSpot`s to put a typeset picture where the text formula is:

1. Render with a `MathText`, so the text reserves room.
2. For each spot, typeset `spot.tex` with quench-tex's typesetter.
3. Move the cursor to (`spot.column`, `spot.line` minus the scroll offset).
4. Print the image scaled to `spot.width` × `spot.height` cells, for example `quench_cli::graphics::kitty::transmit_png(png, id, Placement::cells(w, h))`. See [quench-cli graphics](../quench-cli/api-graphics.md).

A formula that wraps across lines records only where it starts. *Unverified:* there is no such pager in this repository to check against. `loupe`, outside this book, is the intended user.

## 8. Footnotes after their paragraph

```rust
use quench_markdown_term::{FootnotePlacement, Options, render_markdown, to_plain};

let page = "Text with a note[^1] and another[^n].\n\nMore text.\n\n[^1]: The first note.\n[^n]: The second note.\n";
let options = Options { width: 40, footnotes: FootnotePlacement::Inline, ..Options::default() };
assert_eq!(
    to_plain(&render_markdown(page, &options)),
    "Text with a note¹ and another².\n\n  ¹ The first note.\n\n  ² The second note.\n\nMore text.\n"
);
```

Each definition follows the block that first references it, indented and dim. Inside a list item, the definition follows the item's paragraph: `- one[^a]\n- two` gives `• one¹`, then `    ¹ Note.`, then `• two`.

## 9. Narrow terminals and plain ASCII

Tables shrink their widest columns and wrap the cells in them. When even the longest words do not fit, each row becomes `header: value` lines (`tests/unit/table_tests.rs`, at 20 cells):

```text
Name: alpha
Value: 1
Notes: the first
────────────────────
Name: beta
Value: 2
Notes: the second
```

`unicode: false` gives ASCII decoration: `+-|` frames and borders, `* - +` bullets, `[ ]`/`[x]` task boxes, `=`/`-` heading rules, `[^1]` footnote marks (and `[^1]` before each definition at the end) and `^` back-marks. Text from the page itself stays as written. The `[^1]` form keeps a footnote mark apart from a link number ` [1]`.

## 10. Colours

```rust
use quench_markdown_term::{AnsiOptions, Colour, Options, Theme, render_markdown, to_ansi};

let rendered = render_markdown("# Title\n\n`code`", &Options { width: 10, ..Options::default() });
assert_eq!(
    to_ansi(&rendered, &AnsiOptions::default()),
    "\x1b[1;35mTitle\x1b[0m\n\x1b[35m══════════\x1b[0m\n\n\x1b[32mcode\x1b[0m\n"
);

let theme = Theme { link: Some(Colour::BrightCyan), headings: [Some(Colour::Blue), None, None, None, None, None], ..Theme::default() };
let options = AnsiOptions { theme, colour: true, hyperlinks: false };
let _ = to_ansi(&rendered, &options);
```

`colour: false` keeps bold, dim, italic, underline and strikethrough, and drops the theme colours. This suits `NO_COLOR`; the crate does not read `NO_COLOR` itself. Every line ends with `ESC [ 0 m` when it has styles, so lines can be printed one by one, as a pager does.
