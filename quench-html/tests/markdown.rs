//! The `CommonMark` 0.31 specification examples.

#![cfg(feature = "markdown")]
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_html::markdown::{Alignment, Block, Inline, Options, parse, to_html, to_html_with};

#[quench_test::test]
fn passes_the_commonmark_spec() {
    let text = include_str!("fixtures/commonmark-spec.json");
    // The fixture is a JSON array of {"example", "markdown", "html"}; read it without a JSON library.
    let mut failures = Vec::new();
    let mut total = 0;
    for chunk in text.split("{\"example\": ").skip(1) {
        let number: usize = chunk.split(',').next().unwrap().trim().parse().unwrap();
        let markdown = field(chunk, "\"markdown\": ");
        let html = field(chunk, "\"html\": ");
        total += 1;
        let got = to_html(&markdown);
        if got != html {
            failures.push((number, markdown, html, got));
        }
    }
    for (number, markdown, html, got) in failures.iter().take(10) {
        println!("--- example {number}\n{markdown:?}\nexpected {html:?}\n     got {got:?}");
    }
    assert_eq!(total, 652);
    assert!(
        failures.is_empty(),
        "{} of {total} examples fail",
        failures.len()
    );
}

#[quench_test::test]
fn passes_the_gfm_extension_examples() {
    // From cmark-gfm's test/spec.txt (tables, strikethrough) and test/extensions.txt (those, footnotes, task
    // lists); examples that need its extended autolinks are left out.
    let text = include_str!("fixtures/gfm-extensions.json");
    let mut failures = Vec::new();
    let mut total = 0;
    for chunk in text.split("{\"example\": ").skip(1) {
        let number: usize = chunk.split(',').next().unwrap().trim().parse().unwrap();
        let markdown = field(chunk, "\"markdown\": ");
        let html = field(chunk, "\"html\": ");
        total += 1;
        let got = to_html_with(&markdown, Options::GFM);
        if got != html {
            failures.push((number, markdown, html, got));
        }
    }
    for (number, markdown, html, got) in &failures {
        println!("--- gfm example {number}\n{markdown:?}\nexpected {html:?}\n     got {got:?}");
    }
    assert_eq!(total, 24);
    assert!(
        failures.is_empty(),
        "{} of {total} examples fail",
        failures.len()
    );
}

fn all(text: &str) -> String {
    to_html_with(text, Options::ALL)
}

#[quench_test::test]
fn inline_math_follows_githubs_dollar_rules() {
    assert_eq!(
        all("$x^2$\n"),
        "<p><span class=\"math inline\">\\(x^2\\)</span></p>\n"
    );
    assert_eq!(
        all("Euler: $e^{i\\pi} + 1 = 0$.\n"),
        "<p>Euler: <span class=\"math inline\">\\(e^{i\\pi} + 1 = 0\\)</span>.</p>\n"
    );
    // Prices and shell variables are not math.
    assert_eq!(all("costs $5 and $10\n"), "<p>costs $5 and $10</p>\n");
    assert_eq!(all("$ x $\n"), "<p>$ x $</p>\n");
    assert_eq!(all("$x$5\n"), "<p>$x$5</p>\n");
    assert_eq!(all("echo $HOME\n"), "<p>echo $HOME</p>\n");
    // An escaped dollar is a dollar; one inside TeX stays TeX.
    assert_eq!(all("\\$x$\n"), "<p>$x$</p>\n");
    assert_eq!(
        all("$a\\$b$\n"),
        "<p><span class=\"math inline\">\\(a\\$b\\)</span></p>\n"
    );
    // The TeX is escaped, not interpreted as Markdown.
    assert_eq!(
        all("$a<b_*c*$\n"),
        "<p><span class=\"math inline\">\\(a&lt;b_*c*\\)</span></p>\n"
    );
    assert_eq!(
        all("α $β$ γ\n"),
        "<p>α <span class=\"math inline\">\\(β\\)</span> γ</p>\n"
    );
    // `$$…$$` inside a paragraph is inline (kramdown).
    assert_eq!(
        all("so $$a+b$$ here\n"),
        "<p>so <span class=\"math inline\">\\(a+b\\)</span> here</p>\n"
    );
    // A code span keeps its dollars.
    assert_eq!(all("`$x$`\n"), "<p><code>$x$</code></p>\n");
    // Without the option, dollars are text.
    assert_eq!(to_html_with("$x$\n", Options::GFM), "<p>$x$</p>\n");
}

#[quench_test::test]
fn display_math_comes_from_dollar_blocks_paragraphs_and_math_fences() {
    let display = "<div class=\"math display\">\\[\\sum_{i=1}^n i\\]</div>\n";
    assert_eq!(all("$$\n\\sum_{i=1}^n i\n$$\n"), display);
    assert_eq!(all("$$ \\sum_{i=1}^n i $$\n"), display);
    assert_eq!(all("$$\\sum_{i=1}^n i$$\n"), display);
    assert_eq!(all("```math\n\\sum_{i=1}^n i\n```\n"), display);
    assert_eq!(
        all("$$\na \\\\\nb\n$$\n"),
        "<div class=\"math display\">\\[a \\\\\nb\\]</div>\n"
    );
    // In a list item and a block quote.
    assert_eq!(
        all("- $$\n  x\n  $$\n"),
        "<ul>\n<li>\n<div class=\"math display\">\\[x\\]</div>\n</li>\n</ul>\n"
    );
    assert_eq!(
        all("> $$\n> x\n> $$\n"),
        "<blockquote>\n<div class=\"math display\">\\[x\\]</div>\n</blockquote>\n"
    );
    // Not math without the option: a code block stays a code block.
    assert_eq!(
        to_html_with("```math\nx\n```\n", Options::GFM),
        "<pre><code class=\"language-math\">x\n</code></pre>\n"
    );
}

#[quench_test::test]
fn footnotes_are_numbered_by_first_reference_and_unused_ones_dropped() {
    let document = parse(
        "b[^b] a[^A] b[^b]\n\n[^a]: Note a, see[^c].\n[^b]: Note b.\n[^c]: Note c.\n[^unused]: x\n",
        Options::GFM,
    );
    let numbers: Vec<(String, usize, usize)> = document
        .footnotes
        .iter()
        .map(|f| (f.label.clone(), f.number, f.references))
        .collect();
    assert_eq!(
        numbers,
        [
            ("b".to_string(), 1, 2),
            ("a".to_string(), 2, 1),
            ("c".to_string(), 3, 1)
        ]
    );
    let Block::Paragraph(content) = &document.blocks[0] else {
        panic!("a paragraph")
    };
    let refs: Vec<(usize, usize)> = content
        .iter()
        .filter_map(|i| match i {
            Inline::FootnoteReference {
                number, occurrence, ..
            } => Some((*number, *occurrence)),
            _ => None,
        })
        .collect();
    assert_eq!(refs, [(1, 1), (2, 1), (1, 2)]);
    // A reference to nothing stays text; labels match without regard to case.
    assert_eq!(
        to_html_with("x[^none]\n", Options::GFM),
        "<p>x[^none]</p>\n"
    );
}

#[quench_test::test]
fn tables_work_inside_containers_and_give_their_cells() {
    let document = parse(
        "- | a | b |\n  | :- | -: |\n  | 1 | `x\\|y` |\n  | 2 |\n",
        Options::GFM,
    );
    let Block::List(list) = &document.blocks[0] else {
        panic!("a list")
    };
    let Block::Table(table) = &list.items[0].blocks[0] else {
        panic!("a table in the item")
    };
    assert_eq!(table.alignments, [Alignment::Left, Alignment::Right]);
    assert_eq!(table.rows.len(), 2);
    assert_eq!(table.rows[0][1], [Inline::Code("x|y".into())]);
    assert!(table.rows[1][1].is_empty(), "a short row is filled");
    // `---` under a line without a pipe is still a setext heading.
    assert_eq!(to_html_with("abc\n---\n", Options::GFM), "<h2>abc</h2>\n");
    // A paragraph's earlier lines stay a paragraph above the table.
    assert_eq!(
        to_html_with("intro\n| a |\n| - |\n", Options::GFM),
        "<p>intro</p>\n<table>\n<thead>\n<tr>\n<th>a</th>\n</tr>\n</thead>\n</table>\n"
    );
}

#[quench_test::test]
fn task_items_in_a_loose_list_keep_their_paragraph() {
    assert_eq!(
        to_html_with("- [x] done\n\n- [ ] to do\n", Options::GFM),
        "<ul>\n<li>\n<p><input type=\"checkbox\" checked=\"\" disabled=\"\" /> done</p>\n</li>\n<li>\n<p><input type=\"checkbox\" disabled=\"\" /> to do</p>\n</li>\n</ul>\n"
    );
    let document = parse("1. [X] ok\n2. [y] no\n", Options::GFM);
    let Block::List(list) = &document.blocks[0] else {
        panic!("a list")
    };
    assert_eq!(list.marker, '.');
    assert_eq!(list.items[0].task, Some(true));
    assert_eq!(list.items[1].task, None);
}

#[quench_test::test]
fn commonmark_options_leave_the_extensions_as_text() {
    let text = "~~a~~ | b |\n| - |\n- [ ] c\n\nd[^e]\n\n[^e]: f\n";
    let html = to_html(text);
    for literal in ["~~a~~", "[ ] c"] {
        assert!(html.contains(literal), "{literal}: {html}");
    }
    // `[^e]: f` is a link reference definition in plain CommonMark, so `[^e]` is a link, not a footnote.
    assert!(html.contains("<a href=\"f\">^e</a>"), "{html}");
    for tag in ["<table>", "<del>", "<input", "footnote"] {
        assert!(!html.contains(tag), "{tag}: {html}");
    }
}

#[quench_test::test]
fn no_input_makes_it_panic() {
    // Pieces of every construct, combined pseudo-randomly, and every prefix of each combination.
    let pieces = [
        "$",
        "$$",
        "\\",
        "|",
        "| a |",
        "| - |",
        ":-:",
        "[^",
        "[^a]",
        "[^a]:",
        "]",
        "~",
        "~~",
        "`",
        "*",
        "_",
        "[ ]",
        "[x]",
        "- ",
        "> ",
        "    ",
        "\n",
        "\n\n",
        "```math\n",
        "```",
        "α",
        "セン",
        "\t",
        "x",
        " ",
        "#",
        "<b>",
        "&amp;",
        "![",
        "](",
        ")",
        "1. ",
    ];
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    for _ in 0..3000 {
        let mut text = String::new();
        for _ in 0..12 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let pick = usize::try_from(seed % pieces.len() as u64).unwrap();
            text.push_str(pieces[pick]);
        }
        for end in (0..=text.len()).filter(|&i| text.is_char_boundary(i)) {
            let _ = to_html_with(&text[..end], Options::ALL);
        }
    }
}

#[quench_test::test]
fn nesting_thousands_deep_does_not_exhaust_a_small_stack() {
    let deep = [
        ">".repeat(10_000) + " x\n",
        "- ".repeat(5_000) + "x\n",
        "*a ".repeat(20_000) + &" a*".repeat(20_000) + "\n",
        "_a ".repeat(20_000) + &" a_".repeat(20_000) + "\n",
        "~a ".repeat(20_000) + &" a~".repeat(20_000) + "\n",
        "[".repeat(20_000) + "x" + &"](u)".repeat(20_000) + "\n",
        "![".repeat(20_000) + "x" + &"](u)".repeat(20_000) + "\n",
        "[^a]: ".repeat(5_000) + "x\n\n[^a]\n",
    ];
    // Two MiB, as a tokio worker has: the recursive steps after parsing must stay shallow.
    let worker = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            for text in &deep {
                let started = std::time::Instant::now();
                let html = to_html_with(text, Options::ALL);
                assert_ne!(html, "");
                // Linear, not quadratic: deep input must not stall the parser.
                assert!(started.elapsed().as_secs() < 2, "{:?}", started.elapsed());
            }
        })
        .unwrap();
    worker.join().unwrap();
    // An unmatched closer of one kind must not hide a later match of another.
    assert_eq!(
        to_html("*a _b* c_ **d** _e_\n"),
        "<p><em>a _b</em> c_ <strong>d</strong> <em>e</em></p>\n"
    );
    // Within the limit, nesting still works.
    let html = to_html(&(">".repeat(20) + " x\n"));
    assert_eq!(html.matches("<blockquote>").count(), 20);
}

/// The JSON string after `key` in `chunk`.
fn field(chunk: &str, key: &str) -> String {
    let start = chunk.find(key).unwrap() + key.len() + 1;
    let bytes = chunk.as_bytes();
    let mut out = String::new();
    let mut i = start;
    let mut chars = chunk[start..].chars();
    while let Some(c) = chars.next() {
        i += c.len_utf8();
        match c {
            '"' => break,
            '\\' => match chars.next().unwrap() {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'u' => {
                    let hex: String = (0..4).map(|_| chars.next().unwrap()).collect();
                    i += 4;
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
        i += 1;
    }
    let _ = (i, bytes);
    out
}

quench_test::main!();
