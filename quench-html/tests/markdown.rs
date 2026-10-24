//! The `CommonMark` 0.31 specification examples.

#![cfg(feature = "markdown")]
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_html::markdown::to_html;

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
