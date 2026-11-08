---
title: Internals
parent: quench-markdown-term
grand_parent: Crates
nav_order: 3
---

This page explains how a `Document` becomes lines. The private modules are `layout.rs` (blocks), `inline.rs` (inlines and Liquid), `wrap.rs` (word wrapping), `table.rs` (tables) and `width.rs` (cell widths). The one invariant: **no line is wider than `Options::width`** (at least 4). Containers render their content narrower and prefix the lines. When a container would leave its content fewer than 4 cells (`MIN_INNER`), it drops its decoration.

## Page structure

1. The top-level blocks are laid out one under the other, with one blank line between blocks. Tight list items get no blank line between them.
2. With `FootnotePlacement::End`, the definitions of the footnotes not yet shown follow under a rule.
3. With `numbered_links`, a list of the numbered destinations follows under another rule.

Blank lines have no spans. A line is never padded with trailing spaces, except inside frames and tables, which pad to their border.

## Blocks

| Block | Layout |
|---|---|
| Paragraph | Wrapped to the width (see [Wrapping](#wrapping)). A soft break is a space, a hard break a new line. |
| Heading | Bold, `Role::Heading(level)`, wrapped. Level 1 gets a full-width rule `═` underneath, level 2 a rule `─` (ASCII `=` and `-`). Levels 3 to 6 are bold text only. A `{#id}` or `{: #id}` at the end of the text is removed and becomes the anchor. |
| Thematic break | `─` (ASCII `-`) across the full width, `Role::Rule`. |
| Code block (fenced or indented) | Framed with `╭─ lang ───╮ │ … │ ╰───╯`, or `+- lang ---+` in ASCII. `lang` is the first word of the info string, shown in `Role::Marker` when it fits (`label + 6 <= width`). Tabs expand to stops of 4, and control characters are removed. Lines longer than the inner width (`width − 4`) are cut by characters (never inside an emoji sequence or a flag, see [Width](#width)), and each continuation starts with a dim `↪ ` (ASCII `> `). Text is never reflowed. `{% raw %}` / `{% endraw %}` tags are removed. |
| HTML block | As written, dim, `Role::Html`. Each line is cut by characters at the width. |
| Block quote | `│ ` (ASCII `> `) before every line, `Role::Quote`, content 2 cells narrower. A blank line inside gets the bar without its trailing space. |
| List | See [Lists](#lists). |
| Table | See [Tables](#tables). |
| Display math | See [Math](#math). |

### Narrow fallbacks

| Container | Drops its decoration below | Then |
|---|---|---|
| Code frame | 8 cells (`4 + MIN_INNER`) | no frame; rows cut at the width, continuations `↪ ` + `width − 2` cells |
| Quote bar, callout bar | 6 cells | children laid out at the full width, without the bar |
| List item, footnote | indent + 4 > width | the marker on a line of its own (cut by characters if need be), content at the full width |

The fallbacks apply at each level, so 40 nested quotes at 10 cells lose their bars from the depth where they no longer fit (tested in `deep_nesting_fits`).

## Lists

* **Bullets** cycle by list depth: `•`, `◦`, `▪`, then `•` again (ASCII `*`, `-`, `+`). Depth counts list nesting only, so a list inside a quote starts again at `•`.
* **Ordered lists** start at the list's start number and use the list's own delimiter (`.` or `)`). Numbers are right-aligned to the widest one (` 8.`, ` 9.`, `10.`), and the content is indented by the widest marker + 1.
* **Task items** of an unordered list replace the bullet with `☐` / `☑` (ASCII `[ ]` / `[x]`). In an ordered list the box follows the number (`1. ☐ a`).
* **Indent.** Continuation lines of an item are indented to the item's text.
* **Spacing.** Blocks inside an item are spaced like the list: none between them in a tight list, one blank line in a loose one.

From `tests/unit/block_tests.rs` (40 cells):

```text
• one
• two, which is long enough to wrap onto
  a second line at forty cells
  ◦ nested
    ▪ deeper
      • deepest
☐ to do
☑ done

 8. eight
 9. nine
10. ten
```

## Kramdown and Jekyll

**Attribute lists.** An attribute list is a paragraph line that consists only of `{: ... }` (or `{:toc}`). These lines are removed:

* A paragraph made only of attribute lists applies its callout class, if any, to the next block.
* An attribute list on the first line of a paragraph applies to the rest of that paragraph.
* An attribute list on the last line of a paragraph with at least two lines applies to the paragraph itself.

**Callouts.** A class `.note`, `.tip`, `.important`, `.warning`, `.highlight` or `.new` makes the block a callout. It gets a `┃ ` bar (ASCII `| `) in `Role::Callout(kind)` and a bold label line (`Note`, `Tip` ...).

* For a block quote, the quote's children are laid out inside the callout, without the quote bar.
* With the `-title` suffix (`.note-title`), the first paragraph of the quote becomes the label.
* Other classes, such as `.no_toc`, are dropped silently.

**Table of contents.** A list with the single item `TOC`, followed by `{:toc}` on the next line or in the next paragraph, is removed. So is the attribute list after it.

**Liquid in text.**

* `{% include name ... %}` and `{% include_relative name %}` become a dim `[include: name]` (`Role::Html`).
* `{% raw %} ... {% endraw %}` keeps its inside literally.
* Every other `{% ... %}` tag is dropped. A `{%` without a closing `%}` stays as written.
* `{{ site.baseurl }}` is replaced (see `replace_baseurl`), but not inside `{% raw %}`. Other `{{ ... }}` output tags stay as written.

**Liquid in code.** In code spans and code blocks, only the `raw` / `endraw` tags are removed: `{% raw %}`, `{% endraw %}`, `{%raw%}`, `{%endraw%}`, `{%- raw -%}` and `{%- endraw -%}`. Everything else in code is shown as written.

**Heading ids.** `{#id}` or `{: #id}` at the end of a heading's last text sets its anchor. The anchor is made unique on the page by `Slugger::unique`.

**Front matter** is not recognised. Strip it before rendering.

## Control characters

Escape sequences in a page never reach the terminal:

* In text, inline code, inline HTML, link destinations in the link list, image alt text, include names and hook output, tabs, `\n` and `\r` become spaces, and every other `char::is_control` character is removed.
* Code blocks and HTML blocks remove control characters and expand tabs.
* `to_ansi` removes control characters from OSC 8 URLs.

The test `control_characters_never_reach_the_output` checks that ESC and BEL in a paragraph and in a code block are gone.

## Inlines

| Inline | Shown as |
|---|---|
| emphasis / strong / strikethrough | the text with `italic` / `bold` / `strikethrough` |
| code span | the text, `code: true`, `Role::Code` |
| link | the text, underlined, `Role::Link`, `style.link` set; then a dim ` [n]` when numbered |
| image | `[image: alt]`, or `[image]` without alt text, `Role::Image`; then ` [n]` when numbered |
| inline HTML | as written, dim, `Role::Html` |
| footnote reference | superscript digits `¹²³` (ASCII `[^n]`, so it differs from a link number ` [n]`), `Role::Footnote`, linked to `#fn-label` |
| formula | the hook's text, `Role::Math`; or a dim `$tex$` |

The link number, footnote marks and formulas are *glued*: the wrapper keeps them on the same line as the word before. So `docs [1]` never breaks before `[1]`.

## Wrapping

1. The inlines become pieces. Text pieces break at spaces, tabs and newlines. Glued pieces join the word before them. Hard breaks and zero-width marks carry the positions of headings, footnotes and formulas.
2. Words go onto the line while `used + 1 + word` fits, joined by one space in the style of the space.
3. A word that does not fit starts a new line.
4. A word wider than the whole line is broken by clusters (see [Width](#width)), at cell boundaries, so a two-cell character, an emoji sequence or a flag is never split. A word is measured as one text, so an emoji whose U+FE0F is in another style still counts 2.
5. Trailing blank lines are removed.

Spaces are not preserved: runs of spaces collapse to one, which matches how CommonMark text renders.

Marks record the line and column where the text after them lands. Every prefix (quote bar, list indent, callout bar, table border) shifts them, so the spots in `Rendered` are positions in the final lines.

## Tables

1. Each column's *natural* width is the widest cell laid out on one line, header included. Its *least* width is the longest word in the column. Both are at least 1.
2. The borders take `3 × columns + 1` cells. When the sum of the least widths does not fit in what remains, the table is **stacked** (step 5).
3. Otherwise each column starts at its natural width. While the total is too wide, the widest column that is still above its least width gives up one cell, the leftmost first on ties.
4. Cells wrap within their column and are aligned by the delimiter row: `:--` left, `:-:` centre (extra space to the right), `--:` right, none left. Header cells are bold. A row is as tall as its tallest cell. The borders are `┌┬┐ ├┼┤ └┴┘ │ ─`, or `+`, `|`, `-` in ASCII.
5. **Stacked form.** Each row becomes lines of `header: value`, with the header and colon bold, wrapped to the width. Rows are separated by a full-width rule. Special cases:
   * A column with an empty header shows its value alone.
   * An empty value shows `header:` alone.
   * A table without data rows shows the header names once.

From `tests/unit/table_tests.rs` at 40 cells, the left column shrinks and wraps:

```text
┌─────────────────────┬────────┬───────┐
│ Left                │ Centre │ Right │
├─────────────────────┼────────┼───────┤
│ a                   │   b    │     1 │
│ a longer cell that  │ short  │  22.5 │
│ will need to wrap   │        │       │
│ somewhere           │        │       │
└─────────────────────┴────────┴───────┘
```

The parser (`quench_html`) already pads or cuts every row to the header's column count.

## Math

* **Inline.** With `MathMode::Text`, the hook's `inline(tex)` is used when it gives a non-blank result, with control characters removed. Otherwise the formula shows as a dim `$tex$`. Either way the role is `Role::Math`, and the formula is glued to the word before it.
* **Display.** With `MathMode::Text`, the hook's `display(tex)` lines are used when at least one is non-empty and all fit the width. The lines have their tabs expanded and trailing spaces trimmed, and are centred as a block: left margin `(width − widest) / 2`. Otherwise the TeX source (trimmed) shows in a dim frame labelled `tex`, laid out like a code block.

`FormulaSpot` records the area either way. For a pager, the translated text is a placeholder of the right size for a typeset image.

## Footnotes

`quench_html` numbers footnotes from 1 in the order of their first reference, and keeps only the footnotes that are referenced.

* **End.** Under a full-width rule, each definition is `n. text`, or `[^n] text` in ASCII mode (matching the reference mark). The numbers are right-aligned to the widest, and continuation lines are indented. A back-mark `↩` (ASCII `^`), linked to `#fnref-label`, ends the last line when the footnote ends in a paragraph and the mark fits. Otherwise the back-mark gets a line of its own. Definitions are separated by blank lines only when some footnote has more than one block.
* **Inline.** After each block, the footnotes first referenced in it follow as `  ¹ text` (ASCII `  [^1] text`): indented, entirely dim, with no back-mark. This also happens inside list items and quotes, because blocks are laid out recursively. Any footnote not shown by the end of the page is listed at the end, as with `End`.

## Width

`char_width` uses two sorted range tables (`ZERO`, `WIDE`) and a binary search. They are compact ranges, not the Unicode database.

* **Zero width:** combining diacritics (U+0300–036F, 1AB0–1AFF, 1DC0–1DFF, 20D0–20FF, FE20–FE2F), the Hebrew, Arabic, Syriac, Devanagari and Thai marks, Hangul jamo medials and finals (U+1160–11FF), U+200B–200F, U+202A–202E, U+2060–2064, the variation selectors (FE00–FE0F, E0100–E01EF), U+FEFF, the emoji skin-tone modifiers (1F3FB–1F3FF) and tags (E0000–E007F).
* **Two cells:** Hangul jamo leads (1100–115F), CJK (2E80–A4CF and the extension planes 20000–3FFFD), Hangul syllables (AC00–D7A3), CJK compatibility, fullwidth forms (FF00–FF60, FFE0–FFE6), Tangut and kana supplements, and the emoji ranges with default emoji presentation (1F300–1F64F, 1F680–1F6FF, 1F90C–1F9FF, 1FA70–1FAFF and single symbols such as ⌚ ⚡ ✅ ⭐).
* **One cell:** everything else.

`display_width` and the wrapper go by clusters on top of these tables:

| Sequence | Cells | Example |
|---|---|---|
| a character other than whitespace, then U+FE0F | 2 | `❤️` (U+2764 U+FE0F), `#️⃣` |
| a two-cell cluster, U+200D, an emoji or a U+2000–2BFF symbol | 2 for the whole sequence | `👨‍👩‍👧`, `🏃‍♀️`, `🏳️‍🌈` |
| U+200D after anything else | 0, joins nothing | `a‍b` is 2 |
| a skin-tone modifier, U+FE0E, U+FE0F | 0 | `👍🏽` is 2, `☺︎` is 1 |
| two regional indicators | 2 for the pair | `🇵🇱`; a lone `🇵` is 1 |

What is approximate:

* East Asian ambiguous characters count 1.
* Terminals differ among themselves on these cases too.
