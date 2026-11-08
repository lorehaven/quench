use crate::snapshot;
use quench_markdown_term::Options;

const PARAGRAPH: &str = "A paragraph of plain text that goes on long enough to need wrapping at forty cells, with *emphasis*, **strong**, ~~struck~~ and `code spans` in it.\nA soft break joins lines; a hard break  \nstarts a new one. Averyveryverylongwordthatcannotfitonasinglelineoffortycellsatall ends it.\n";

/// Wrapping at spaces, styles without markup, soft and hard breaks, a word longer than the line.
#[quench_test::test]
fn paragraph() {
    snapshot(
        PARAGRAPH,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
A paragraph of plain text that goes on
long enough to need wrapping at forty
cells, with emphasis, strong, struck and
code spans in it. A soft break joins
lines; a hard break
starts a new one.
Averyveryverylongwordthatcannotfitonasin
glelineoffortycellsatall ends it.
"#,
    );
    snapshot(
        PARAGRAPH,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
A paragraph of plain text that goes on long enough to need wrapping at forty
cells, with emphasis, strong, struck and code spans in it. A soft break joins
lines; a hard break
starts a new one.
Averyveryverylongwordthatcannotfitonasinglelineoffortycellsatall ends it.
"#,
    );
    snapshot(
        PARAGRAPH,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
A paragraph of plain text that goes on long enough to need wrapping at forty cells, with emphasis, strong, struck and
code spans in it. A soft break joins lines; a hard break
starts a new one. Averyveryverylongwordthatcannotfitonasinglelineoffortycellsatall ends it.
"#,
    );
}

const HEADINGS: &str = "# One\n\nText.\n\n## Two\n\n### Three\n\n#### Four\n\nEnd.\n";

/// h1 and h2 with a full-width rule under, h3+ bold only, one blank line between blocks.
#[quench_test::test]
fn headings() {
    snapshot(
        HEADINGS,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
One
════════════════════════════════════════

Text.

Two
────────────────────────────────────────

Three

Four

End.
"#,
    );
    snapshot(
        HEADINGS,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
One
════════════════════════════════════════════════════════════════════════════════

Text.

Two
────────────────────────────────────────────────────────────────────────────────

Three

Four

End.
"#,
    );
    snapshot(
        HEADINGS,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
One
════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════

Text.

Two
────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────

Three

Four

End.
"#,
    );
}

const CODE: &str = "```rust\nfn main() {\n\tlet x = 1;\n    println!(\"a long line of code that is wider than forty cells, and wider than eighty too, for sure\");\n}\n```\n\n    indented code\n";

/// A frame with the language, tabs expanded, long lines wrapped with a continuation mark.
#[quench_test::test]
fn code() {
    snapshot(
        CODE,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
╭─ rust ───────────────────────────────╮
│ fn main() {                          │
│     let x = 1;                       │
│     println!("a long line of code th │
│ ↪ at is wider than forty cells, and  │
│ ↪ wider than eighty too, for sure"); │
│ }                                    │
╰──────────────────────────────────────╯

╭──────────────────────────────────────╮
│ indented code                        │
╰──────────────────────────────────────╯
"#,
    );
    snapshot(
        CODE,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
╭─ rust ───────────────────────────────────────────────────────────────────────╮
│ fn main() {                                                                  │
│     let x = 1;                                                               │
│     println!("a long line of code that is wider than forty cells, and wider  │
│ ↪ than eighty too, for sure");                                               │
│ }                                                                            │
╰──────────────────────────────────────────────────────────────────────────────╯

╭──────────────────────────────────────────────────────────────────────────────╮
│ indented code                                                                │
╰──────────────────────────────────────────────────────────────────────────────╯
"#,
    );
    snapshot(
        CODE,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
╭─ rust ───────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ fn main() {                                                                                                          │
│     let x = 1;                                                                                                       │
│     println!("a long line of code that is wider than forty cells, and wider than eighty too, for sure");             │
│ }                                                                                                                    │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯

╭──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ indented code                                                                                                        │
╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
"#,
    );
}

const QUOTE: &str = "> A quote that is long enough to wrap at forty cells, around the bar.\n>\n> > A nested quote.\n>\n> - with\n> - a list\n";

/// A bar before every line, nested quotes and lists inside.
#[quench_test::test]
fn quote() {
    snapshot(
        QUOTE,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
│ A quote that is long enough to wrap at
│ forty cells, around the bar.
│
│ │ A nested quote.
│
│ • with
│ • a list
"#,
    );
    snapshot(
        QUOTE,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
│ A quote that is long enough to wrap at forty cells, around the bar.
│
│ │ A nested quote.
│
│ • with
│ • a list
"#,
    );
    snapshot(
        QUOTE,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
│ A quote that is long enough to wrap at forty cells, around the bar.
│
│ │ A nested quote.
│
│ • with
│ • a list
"#,
    );
}

const LISTS: &str = "- one\n- two, which is long enough to wrap onto a second line at forty cells\n  - nested\n    - deeper\n      - deepest\n- [ ] to do\n- [x] done\n\n8. eight\n9. nine\n10. ten\n\n1) loose\n\n   with a second paragraph\n\n2) item\n";

/// Bullets by depth, task boxes, ordered numbers right-aligned, tight and loose lists.
#[quench_test::test]
fn lists() {
    snapshot(
        LISTS,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
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

1) loose

   with a second paragraph

2) item
"#,
    );
    snapshot(
        LISTS,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
• one
• two, which is long enough to wrap onto a second line at forty cells
  ◦ nested
    ▪ deeper
      • deepest
☐ to do
☑ done

 8. eight
 9. nine
10. ten

1) loose

   with a second paragraph

2) item
"#,
    );
    snapshot(
        LISTS,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
• one
• two, which is long enough to wrap onto a second line at forty cells
  ◦ nested
    ▪ deeper
      • deepest
☐ to do
☑ done

 8. eight
 9. nine
10. ten

1) loose

   with a second paragraph

2) item
"#,
    );
}

const RULE_HTML: &str = "Above.\n\n---\n\n<div class=\"box\">\n  <b>raw</b>\n</div>\n\nBelow with <kbd>inline</kbd> HTML.\n";

/// A thematic break across the width, raw HTML as written.
#[quench_test::test]
fn rule_html() {
    snapshot(
        RULE_HTML,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
Above.

────────────────────────────────────────

<div class="box">
  <b>raw</b>
</div>

Below with <kbd>inline</kbd> HTML.
"#,
    );
    snapshot(
        RULE_HTML,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
Above.

────────────────────────────────────────────────────────────────────────────────

<div class="box">
  <b>raw</b>
</div>

Below with <kbd>inline</kbd> HTML.
"#,
    );
    snapshot(
        RULE_HTML,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
Above.

────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────

<div class="box">
  <b>raw</b>
</div>

Below with <kbd>inline</kbd> HTML.
"#,
    );
}

const NESTED: &str = "> 1. first item in a quote, long enough to wrap\n>    > a quote in the item\n>\n>    ```\n>    code in both\n>    ```\n> 2. second\n";

/// A list in a quote, with a quote and a code block in the item.
#[quench_test::test]
fn nested() {
    snapshot(
        NESTED,
        &Options {
            width: 40,
            ..Options::default()
        },
        r#"
│ 1. first item in a quote, long enough
│    to wrap
│
│    │ a quote in the item
│
│    ╭─────────────────────────────────╮
│    │ code in both                    │
│    ╰─────────────────────────────────╯
│
│ 2. second
"#,
    );
    snapshot(
        NESTED,
        &Options {
            width: 80,
            ..Options::default()
        },
        r#"
│ 1. first item in a quote, long enough to wrap
│
│    │ a quote in the item
│
│    ╭─────────────────────────────────────────────────────────────────────────╮
│    │ code in both                                                            │
│    ╰─────────────────────────────────────────────────────────────────────────╯
│
│ 2. second
"#,
    );
    snapshot(
        NESTED,
        &Options {
            width: 120,
            ..Options::default()
        },
        r#"
│ 1. first item in a quote, long enough to wrap
│
│    │ a quote in the item
│
│    ╭─────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│    │ code in both                                                                                                    │
│    ╰─────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
│
│ 2. second
"#,
    );
}

const ASCII: &str = "# Title\n\n- a\n  - b\n- [x] c\n\n> quote[^1]\n\n```sh\nls\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n[^1]: note\n";

/// Without Unicode: ASCII bullets, frames, borders, rules and footnote marks.
#[quench_test::test]
fn ascii() {
    snapshot(
        ASCII,
        &Options {
            width: 40,
            unicode: false,
            ..Options::default()
        },
        r#"
Title
========================================

* a
  - b
[x] c

> quote[^1]

+- sh ---------------------------------+
| ls                                   |
+--------------------------------------+

+---+---+
| a | b |
+---+---+
| 1 | 2 |
+---+---+

----------------------------------------

----------------------------------------
[^1] note ^
"#,
    );
    snapshot(
        ASCII,
        &Options {
            width: 80,
            unicode: false,
            ..Options::default()
        },
        r#"
Title
================================================================================

* a
  - b
[x] c

> quote[^1]

+- sh -------------------------------------------------------------------------+
| ls                                                                           |
+------------------------------------------------------------------------------+

+---+---+
| a | b |
+---+---+
| 1 | 2 |
+---+---+

--------------------------------------------------------------------------------

--------------------------------------------------------------------------------
[^1] note ^
"#,
    );
    snapshot(
        ASCII,
        &Options {
            width: 120,
            unicode: false,
            ..Options::default()
        },
        r#"
Title
========================================================================================================================

* a
  - b
[x] c

> quote[^1]

+- sh -----------------------------------------------------------------------------------------------------------------+
| ls                                                                                                                   |
+----------------------------------------------------------------------------------------------------------------------+

+---+---+
| a | b |
+---+---+
| 1 | 2 |
+---+---+

------------------------------------------------------------------------------------------------------------------------

------------------------------------------------------------------------------------------------------------------------
[^1] note ^
"#,
    );
}
