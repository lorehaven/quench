use crate::assert_fits;
use quench_markdown_term::{FootnotePlacement, MathMode, Options, render_markdown};
use std::path::{Path, PathBuf};

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            markdown_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "md") {
            out.push(path);
        }
    }
}

fn variants(width: usize) -> [Options; 3] {
    [
        Options {
            width,
            ..Options::default()
        },
        Options {
            width,
            unicode: false,
            footnotes: FootnotePlacement::Inline,
            ..Options::default()
        },
        Options {
            width,
            numbered_links: false,
            math: MathMode::Source,
            baseurl: "/forge".into(),
            ..Options::default()
        },
    ]
}

/// Every page of the forge book (when it is checked out beside quench) fits at 40, 80 and 120 cells.
#[quench_test::test]
fn the_forge_book_fits_its_widths() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../forge/docs");
    let mut files = Vec::new();
    markdown_files(&dir, &mut files);
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        for width in [40, 80, 120] {
            for options in variants(width) {
                let rendered = render_markdown(&text, &options);
                assert_fits(&rendered, width, &file.display().to_string());
            }
        }
    }
}

/// A small deterministic generator (xorshift), so the generated pages are the same on every run.
struct Random(u64);

impl Random {
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % 1_000_003).unwrap()
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.next() % items.len()]
    }
}

const FRAGMENTS: &[&str] = &[
    "# ",
    "## ",
    "### ",
    "> ",
    "> > ",
    "- ",
    "* ",
    "1. ",
    "10) ",
    "  ",
    "    ",
    "\t",
    "```",
    "```rust\n",
    "~~~",
    "| a | b |\n",
    "|---|:-:|\n",
    "| 1 | 2 |\n",
    "---\n",
    "***",
    "[^1]",
    "[^x]",
    "[^1]: note ",
    "[^x]: other\n",
    "$x$",
    "$$",
    "$$\n",
    "**",
    "*",
    "_",
    "__",
    "`",
    "~~",
    "[ ] ",
    "[x] ",
    "\\",
    "\n",
    "\n",
    "\n\n",
    "text ",
    "word ",
    "averyveryverylongwordwithoutanyspacesatallthatgoesonandon ",
    "セン ",
    "e\u{301} ",
    "😀 ",
    "❤\u{FE0F}",
    "👨\u{200D}👩\u{200D}👧 ",
    "🇵🇱",
    "👍🏽",
    "☺\u{FE0E}",
    "\u{FE0F}",
    "\u{200D}",
    "{: .note }\n",
    "{:toc}\n",
    "{% include a.html %}",
    "{% raw %}",
    "{% endraw %}",
    "{{ site.baseurl }}",
    "<div>",
    "</div>\n",
    "<b>",
    "[a](b)",
    "[a]({{ site.baseurl }}/x)",
    "<https://e.com>",
    "![i](j.png)",
    "{#id}",
    "&amp;",
    "    code\n",
    "\u{1b}[1m",
    "\r\n",
];

/// Generated pages of random fragments neither panic nor overflow any width.
#[quench_test::test]
fn generated_pages_fit_and_do_not_panic() {
    let mut random = Random(0x2545_F491_4F6C_DD1D);
    for _ in 0..600 {
        let length = random.next() % 120;
        let page: String = (0..length).map(|_| random.pick(FRAGMENTS)).collect();
        let width = 1 + random.next() % 60;
        for options in variants(width) {
            let rendered = render_markdown(&page, &options);
            assert_fits(&rendered, width.max(4), &format!("{page:?} at {width}"));
        }
    }
}

/// Deep nesting drops decoration instead of overflowing.
#[quench_test::test]
fn deep_nesting_fits() {
    let page = format!("{}text that is long\n", "> ".repeat(40));
    let mut lists = String::new();
    for i in 0..30 {
        lists.push_str(&"  ".repeat(i));
        lists.push_str("- item\n");
    }
    for width in [4, 10, 40] {
        for text in [&page, &lists] {
            assert_fits(
                &render_markdown(
                    text,
                    &Options {
                        width,
                        ..Options::default()
                    },
                ),
                width,
                text,
            );
        }
    }
}
