---
title: API reference
parent: quench-markdown-term
grand_parent: Crates
nav_order: 2
---

Every public item is exported from the crate root. The modules `ansi`, `inline`, `layout`, `model`, `slug`, `table`, `tex` (feature `tex`), `width` and `wrap` are private. This page groups the items by where they are defined. Signatures are copied from `quench-markdown-term/src`. There is no `unsafe`. Every value type here is plain data and is `Send + Sync`, except `TexMath` (see below).

## Rendering (crate root)

### `struct Options`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub width: usize,
    pub unicode: bool,
    pub footnotes: FootnotePlacement,
    pub math: MathMode,
    pub baseurl: String,
    pub numbered_links: bool,
}
impl Default for Options
```

| Field | Default | Meaning |
|---|---|---|
| `width` | `80` | Width in terminal cells. Every line fits in it. Values under 4 are laid out at 4. |
| `unicode` | `true` | Box drawing, bullets `• ◦ ▪`, task boxes `☐ ☑`, superscript footnote marks, `↩`, `↪`. When `false`, ASCII is used instead: `+-|`, `* - +`, `[ ]`/`[x]`, footnote marks `[^1]` (and `[^1]` before each definition at the end), `^`, `> `. |
| `footnotes` | `FootnotePlacement::End` | Where footnote definitions go. |
| `math` | `MathMode::Text` | Formulas through the hook, or always as source. |
| `baseurl` | `""` | What `{{ site.baseurl }}` stands for in link destinations and text. |
| `numbered_links` | `true` | A dim ` [n]` after each link whose destination differs from its text, with the destinations listed `[n] url` under a rule at the end. Meant for output without hyperlinks. Turn it off when writing with `AnsiOptions::hyperlinks`. |

Rules for numbering, from `inline.rs`:

* A link is numbered when its shown text differs from its destination, and also differs from the destination with `mailto:` removed.
* An image is always numbered.
* Empty destinations and destinations starting with `#` are never numbered.
* Two links to the same destination share one number.

### `enum FootnotePlacement`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FootnotePlacement {
    #[default] End,   // at the end of the page under a rule, each with a back-mark ↩
    Inline,           // each in a dim indented block right after the block holding its first reference
}
```

### `enum MathMode`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MathMode {
    #[default] Text,  // as text from the MathText hook, or as source where it gives none
    Source,           // always TeX source: inline as a dim `$tex$`, display in a dim frame labelled `tex`
}
```

### `trait MathText`

```rust
pub trait MathText {
    fn inline(&self, tex: &str) -> Option<String>;
    fn display(&self, tex: &str) -> Option<Vec<String>>;
}
```

`MathText` turns TeX into terminal text. `tex` is the formula without its `$` delimiters.

* `inline` returns one line.
* `display` returns lines, which may be laid out in two dimensions. The renderer centres them as a block.

Where the hook returns `None`, the formula's source is shown whole, never half-translated. The source is also shown when:

* `inline` returns a result that is empty or only spaces;
* `display` returns no non-empty line;
* any display line is wider than the available width.

Control characters in the hook's output are removed. Tabs in display lines are expanded and trailing spaces trimmed. The hook takes `&self`; a hook that keeps state needs interior mutability, as `TexMath` has.

### `struct SourceMath`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceMath;
impl MathText for SourceMath   // both methods return None
```

### `fn render`, `fn render_with_math`, `fn render_markdown`

```rust
#[must_use] pub fn render(document: &Document, options: &Options) -> Rendered
#[must_use] pub fn render_with_math(document: &Document, options: &Options, math: &dyn MathText) -> Rendered
#[must_use] pub fn render_markdown(markdown: &str, options: &Options) -> Rendered
```

`Document` is `quench_html::markdown::Document` (see [quench-html](../quench-html/index.md)).

* `render(document, options)` is `render_with_math(document, options, &SourceMath)`, so formulas show as source.
* `render_with_math` lays the document out with formulas through `math`. No panic is documented, and pages generated from random fragments are tested not to panic (see [Testing](testing.md)).
* `render_markdown(markdown, options)` first runs `replace_baseurl(markdown, &options.baseurl)` on the whole source, as Jekyll does. It then parses with `quench_html::markdown::parse(&source, quench_html::markdown::Options::ALL)` and calls `render`. Because it uses `render`, it never translates formulas. The replacement runs before parsing, so it also changes code spans and code blocks, as Jekyll does; the inside of `{% raw %}` ... `{% endraw %}` is kept as written.

None of the three strips front matter.

## Output model (`model.rs`)

### `struct Rendered`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rendered {
    pub lines: Vec<Line>,             // each at most the width; blank lines have no spans
    pub links: Vec<LinkTarget>,       // links, images and footnote marks; Style::link indexes this
    pub headings: Vec<HeadingSpot>,   // in order of line
    pub footnotes: Vec<FootnoteSpot>, // one per footnote of the Document, in its order
    pub formulas: Vec<FormulaSpot>,   // in the order they were laid out
}

impl Rendered {
    #[must_use] pub fn heading_line(&self, slug: &str) -> Option<usize>
}
```

`heading_line(slug)` returns the line where the first heading with that slug starts, for following a `#slug` link.

### `struct Line` and `struct Span`

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line { pub spans: Vec<Span> }

impl Line {
    #[must_use] pub fn width(&self) -> usize      // display_width of the line's text
    #[must_use] pub fn text(&self) -> String      // the text without styles
    #[must_use] pub fn is_empty(&self) -> bool    // every span's text is empty
    pub fn push(&mut self, text: &str, style: Style)   // appends; joins the last span when the style is equal; ignores ""
    pub fn extend(&mut self, other: &Self)             // push of each span of `other`
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Span { pub text: String, pub style: Style }
```

### `struct Style`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub dim: bool,
    pub code: bool,           // code span or block (only a theme colour marks it)
    pub role: Role,
    pub link: Option<usize>,  // index into Rendered::links
}
```

What sets each attribute:

* Emphasis sets `italic`, strong sets `bold`, strikethrough sets `strikethrough`.
* Links are `underline` with `role: Link`.
* Headings, table header cells and callout labels are `bold`.
* The following are `dim`: the ` [n]` link numbers, the link list, raw HTML, includes, formula sources, the code continuation mark `↪`, and inline footnote definitions.

### `enum Role`

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Role {
    #[default] None,       // body text
    Heading(u8),           // level 1-6: the heading and its rule
    Link,
    Image,
    Code,
    Quote,                 // a block quote's bar
    Rule,                  // thematic breaks, frames, table borders
    Marker,                // list bullets, numbers, task boxes; a code block's language
    Math,
    Html,                  // raw HTML and the Liquid shown ([include: ...])
    Footnote,              // footnote references, back-marks and numbers
    Callout(CalloutKind),  // a callout's bar and label
}
```

### `enum CalloutKind`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CalloutKind { Note, Tip, Important, Warning, Highlight, New }

impl CalloutKind {
    #[must_use] pub fn from_class(class: &str) -> Option<Self>   // "note", "tip", "important", "warning", "highlight", "new"
    #[must_use] pub const fn label(self) -> &'static str          // "Note", "Tip", "Important", "Warning", "Highlight", "New"
}
```

`from_class` is case-sensitive. The `-title` suffix (`important-title`) is handled by the renderer, not by `from_class`.

### `struct LinkTarget` and `enum LinkKind`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    pub destination: String,     // with {{ site.baseurl }} replaced; #fn-label / #fnref-label for footnotes
    pub title: String,
    pub kind: LinkKind,
    pub number: Option<usize>,   // the [n], when links are numbered
}

impl LinkTarget {
    #[must_use] pub fn is_url(&self) -> bool
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Link,
    Image,
    Footnote(usize),       // a reference to footnote n: goes to its definition
    FootnoteBack(usize),   // a definition's back-mark: goes to footnote n's first reference
}
```

`is_url()` is true for `Link` and `Image` when the destination has a URL scheme. The scheme is the part before the first `:`. It must be at least 2 characters, start with an ASCII letter, and contain only ASCII alphanumerics, `+`, `-` and `.`. So `https:` and `mailto:` count, while relative paths, `#anchor`s and `C:` do not. Only these links become OSC 8 hyperlinks.

There is one `LinkTarget` per link or image occurrence, one per footnote's references (shared by all of them), and one per footnote's back-mark.

### Spots

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingSpot {
    pub level: u8,
    pub text: String,   // without markup, trimmed, baseurl replaced
    pub slug: String,   // the {#id} it gives, or the slug of its text; unique on the page
    pub line: usize,    // first line of the heading
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootnoteSpot {
    pub number: usize,
    pub label: String,
    pub definition_line: usize,     // the line its definition starts on (0 if never laid out)
    pub reference_lines: Vec<usize>,// lines of its references, sorted
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaSpot {
    pub line: usize,     // first line it takes
    pub column: usize,   // cell it starts at on that line
    pub width: usize,    // cells across
    pub height: usize,   // lines
    pub tex: String,
    pub display: bool,   // true for a display formula (a Block::Math); false for every formula inside a paragraph
}
```

For an inline formula, `width` is the display width of the shown text (the translation, or `$tex$`) and `height` is 1. For a translated display formula, they are the widest line and the line count, and `column` is the left edge of the centred block. For a display formula shown as source, they are the frame's width and height, at column 0.

A heading whose content is empty after layout is not recorded.

## Writers (`ansi.rs`)

### `fn to_ansi` and `fn to_plain`

```rust
#[must_use] pub fn to_ansi(rendered: &Rendered, options: &AnsiOptions) -> String
#[must_use] pub fn to_plain(rendered: &Rendered) -> String
```

`to_plain` writes each line's text followed by `\n`, with no escape sequences.

`to_ansi` writes each line followed by `\n`:

* Spans in a new style are preceded by `ESC [ 0 m` (when a style was active) and then `ESC [ codes m`. The codes come in the order bold `1`, dim `2`, italic `3`, underline `4`, strikethrough `9`, then the theme colour.
* A line with any style ends with `ESC [ 0 m`, so each line stands alone.
* With `hyperlinks`, spans whose `LinkTarget::is_url()` is true are wrapped in OSC 8: `ESC ] 8 ; ; url ESC \` before and `ESC ] 8 ; ; ESC \` after. Control characters are removed from the URL. A hyperlink never spans a line break: it is closed at the end of each line and reopened on the next.

Example (from the tests):

```rust
use quench_markdown_term::{AnsiOptions, Options, render_markdown, to_ansi, to_plain};

let rendered = render_markdown("Go [there](https://example.com) now.",
    &Options { width: 40, numbered_links: false, ..Options::default() });
let options = AnsiOptions { hyperlinks: true, colour: false, ..AnsiOptions::default() };
assert_eq!(
    to_ansi(&rendered, &options),
    "Go \x1b]8;;https://example.com\x1b\\\x1b[4mthere\x1b]8;;\x1b\\\x1b[0m now.\n"
);
assert_eq!(to_plain(&rendered), "Go there now.\n");
```

### `struct AnsiOptions`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiOptions {
    pub theme: Theme,
    pub hyperlinks: bool,   // default false
    pub colour: bool,       // default true; false keeps bold/italic/underline/strikethrough/dim only
}
impl Default for AnsiOptions   // Theme::default(), hyperlinks: false, colour: true
```

### `enum Colour`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Black, Red, Green, Yellow, Blue, Magenta, Cyan, White,
    BrightBlack, BrightRed, BrightGreen, BrightYellow, BrightBlue, BrightMagenta, BrightCyan, BrightWhite,
}
impl Colour {
    #[must_use] pub const fn foreground(self) -> u8   // 30-37, then 90-97
}
```

These are the 16 basic colours, which every terminal maps to its own palette. There is no 256-colour or RGB form.

### `struct Theme`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub headings: [Option<Colour>; 6],   // levels 1 to 6
    pub link: Option<Colour>,
    pub image: Option<Colour>,
    pub code: Option<Colour>,
    pub quote: Option<Colour>,
    pub rule: Option<Colour>,
    pub marker: Option<Colour>,
    pub math: Option<Colour>,
    pub html: Option<Colour>,
    pub footnote: Option<Colour>,
    pub note: Option<Colour>,
    pub tip: Option<Colour>,
    pub important: Option<Colour>,
    pub warning: Option<Colour>,
    pub highlight: Option<Colour>,
    pub new: Option<Colour>,
}
impl Theme {
    #[must_use] pub fn colour(&self, role: Role) -> Option<Colour>
}
impl Default for Theme
```

`None` leaves the terminal's own foreground. `colour(role)` maps `Role::None` to `None` and `Heading(level)` to `headings[level.clamp(1, 6) - 1]`. Each other role maps to its own field, and each `Callout` kind to the field of the same name.

The default theme is chosen to read on dark and light backgrounds: no white, black or yellow text, and grey only for decoration.

| Field | Default |
|---|---|
| `headings` | `Magenta`, `Cyan`, `Green`, then `None` for 4-6 |
| `link` | `Blue` |
| `image` | `Magenta` |
| `code` | `Green` |
| `quote` | `BrightBlack` |
| `rule` | `BrightBlack` |
| `marker` | `Cyan` |
| `math` | `Magenta` |
| `html` | `None` |
| `footnote` | `Cyan` |
| `note` | `Blue` |
| `tip` | `Green` |
| `important` | `Magenta` |
| `warning` | `Red` |
| `highlight` | `Cyan` |
| `new` | `Green` |

## Width (`width.rs`)

```rust
#[must_use] pub fn char_width(c: char) -> usize
#[must_use] pub fn display_width(text: &str) -> usize
```

`char_width` returns:

* 0 for C0 controls (below U+0020), DEL and C1 controls (U+007F to U+009F);
* 1 for everything else below U+0300;
* 0 for the zero-width ranges (combining marks, variation selectors, zero-width spaces and joiners, bidi controls, emoji skin-tone modifiers, tags);
* 2 for the wide ranges (East Asian Wide and Fullwidth, Hangul, emoji shown as emoji by default);
* 1 for the rest.

`char_width` sees one character, so it knows nothing of emoji sequences. `display_width` splits the text into clusters (a character with the zero-width characters after it, an emoji sequence, a flag) and adds their cells:

* a visible character other than whitespace followed by U+FE0F (emoji presentation) takes 2 cells (`❤️`, `#️⃣`);
* after a two-cell cluster, U+200D (ZWJ) joins the next character when it is an emoji or a symbol in U+2000–2BFF; the joined character adds nothing, so `👨‍👩‍👧` takes 2. A ZWJ after anything else is zero-width and joins nothing (`a‍b` is 2);
* skin-tone modifiers (U+1F3FB–1F3FF) and U+FE0E/U+FE0F take 0 (`👍🏽` is 2, `☺︎` is 1);
* two regional indicators make a flag of 2 cells (`🇵🇱`); a lone one takes 1.

Every other cluster takes the `char_width` of its first character. The layout measures and breaks text by the same clusters, so it never splits one across lines. See [Internals](internals.md#width) for what the tables cover.

```rust
use quench_markdown_term::{char_width, display_width};

assert_eq!(display_width("abc"), 3);
assert_eq!(display_width("セン"), 4);
assert_eq!(display_width("e\u{301}"), 1);
assert_eq!(display_width("🚀 go"), 5);
assert_eq!(display_width("\u{2764}\u{FE0F}"), 2);
assert_eq!(display_width("👨\u{200D}👩\u{200D}👧"), 2);
assert_eq!(display_width("🇵🇱"), 2);
assert_eq!(char_width('\u{200B}'), 0);
```

## Slugs (`slug.rs`)

```rust
#[must_use] pub fn slug(text: &str) -> String

#[derive(Debug, Clone, Default)]
pub struct Slugger { /* used: HashSet<String>, counts: HashMap<String, usize> */ }
impl Slugger {
    #[must_use] pub fn new() -> Self
    pub fn slug(&mut self, text: &str) -> String
    pub fn unique(&mut self, anchor: &str) -> String
}
```

* `slug(text)` builds the anchor of a heading reading `text`, as GitHub and kramdown make them. It trims the text and then goes character by character: letters, digits (`char::is_alphanumeric`, so not only ASCII), `-` and `_` are kept and lowercased, each space becomes `-`, and everything else is dropped. Two spaces give two dashes.
* `Slugger::slug(text)` is `unique(&slug(text))`.
* `Slugger::unique(anchor)` returns `anchor` the first time. After that it returns `anchor-1`, `anchor-2` ..., skipping any candidate already used. The renderer uses one `Slugger` per page, and a heading's own `{#id}` goes through `unique`, without slugging.

```rust
use quench_markdown_term::{Slugger, slug};

assert_eq!(slug("Hello, World!"), "hello-world");
assert_eq!(slug("`quench-html` and C++"), "quench-html-and-c");
assert_eq!(slug("Ünïcode Straße"), "ünïcode-straße");
let mut slugger = Slugger::new();
assert_eq!(slugger.slug("Usage"), "usage");
assert_eq!(slugger.slug("Usage"), "usage-1");
assert_eq!(slugger.slug("Usage 1"), "usage-1-1");
```

## Jekyll helper (`inline.rs`)

```rust
#[must_use]
pub fn replace_baseurl(text: &str, baseurl: &str) -> String
```

`replace_baseurl` replaces every `{{ site.baseurl }}`, `{{site.baseurl}}`, `%7B%7B%20site.baseurl%20%7D%7D` and `%7B%7Bsite.baseurl%7D%7D` in `text` with `baseurl`, except inside `{% raw %}` ... `{% endraw %}` (any spacing or `-` trim marks in the tags), where Jekyll keeps it as written. The raw tags themselves are kept, for the renderer to remove. A `{% raw %}` without `{% endraw %}` protects the rest of the text. Other spacing variants are not replaced, such as `{{site.baseurl }}`.

```rust
use quench_markdown_term::replace_baseurl;
assert_eq!(replace_baseurl("[a]({{ site.baseurl }}/x)", "/book"), "[a](/book/x)");
assert_eq!(
    replace_baseurl("{% raw %}{{ site.baseurl }}{% endraw %}", "/book"),
    "{% raw %}{{ site.baseurl }}{% endraw %}",
);
```

## `TexMath` (feature `tex`, `tex.rs`)

```rust
#[cfg(feature = "tex")]
#[derive(Debug, Default)]
pub struct TexMath { /* macros: RefCell<quench_tex::Macros> */ }

impl TexMath {
    #[must_use] pub fn new() -> Self
}
impl MathText for TexMath
```

`TexMath` is the `MathText` of [quench-tex](../quench-tex/index.md):

* `inline(tex)` returns `quench_tex::to_unicode` of the formula.
* `display(tex)` returns `quench_tex::to_text_block(&formula).lines`. These are two-dimensional: stacked fractions, limits above and below, tall delimiters.

Both read the formula with `quench_tex::parse_with(tex, &mut macros)`. Definitions made by a formula (`\newcommand`, `\def`, `\DeclareMathOperator`) are therefore kept for the formulas after it on the same `TexMath`, as on a page typeset by MathJax. Use one `TexMath` per page. A formula that fails to parse returns `None`, which shows its source, and changes no macros.

`TexMath` is `Send` but not `Sync`, because it holds a `RefCell`. Use it from one thread at a time.
