use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use quench_mail::mime::*;

/// Decodes `=?UTF-8?B?...?=` words back to text.
fn decode_words(words: &[String]) -> String {
    words
        .iter()
        .map(|w| {
            let inner = w
                .strip_prefix("=?UTF-8?B?")
                .and_then(|w| w.strip_suffix("?="))
                .expect("encoded word shape");
            String::from_utf8(STANDARD.decode(inner).unwrap()).expect("valid UTF-8 per word")
        })
        .collect()
}

/// A reference quoted-printable decoder, to round-trip the encoder against.
fn qp_decode(encoded: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = encoded.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' {
            if bytes[i + 1..].starts_with(b"\r\n") {
                i += 3; // soft break
                continue;
            }
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
            out.push(u8::from_str_radix(hex, 16).expect("hex escape"));
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

#[test]
fn normalize_crlf_handles_every_line_ending() {
    assert_eq!(normalize_crlf("a\nb"), "a\r\nb");
    assert_eq!(normalize_crlf("a\r\nb"), "a\r\nb");
    assert_eq!(normalize_crlf("a\rb"), "a\r\nb");
    assert_eq!(normalize_crlf("a\r\n\nb\r"), "a\r\n\r\nb\r\n");
    assert_eq!(normalize_crlf(""), "");
}

#[test]
fn plain_header_text_is_printable_ascii() {
    assert!(is_plain_header_text("Hello, world! \t~"));
    assert!(!is_plain_header_text("caf\u{e9}"));
    assert!(!is_plain_header_text("line\nbreak"));
    assert!(!is_plain_header_text("bell\u{7}"));
}

#[test]
fn encoded_words_round_trip_and_stay_within_75_characters() {
    for text in [
        "Zażółć gęślą jaźń",
        "日本語のメールの件名がとても長い場合でも正しく分割されること",
        &"é".repeat(200),
        "mixed ascii and 😀 emoji 😀😀😀 that goes on for a while and a while more",
    ] {
        let words = encoded_words(text);
        assert!(words.iter().all(|w| w.len() <= 75), "{words:?}");
        assert_eq!(decode_words(&words), text);
    }
    assert!(encoded_words("").is_empty());
}

#[test]
fn unstructured_leaves_plain_text_alone_and_encodes_the_rest() {
    assert_eq!(unstructured("Confirm your address"), "Confirm your address");
    let encoded = unstructured("Potwierdź adres");
    assert!(encoded.starts_with("=?UTF-8?B?"));
    assert!(is_plain_header_text(&encoded));
}

#[test]
fn phrase_quotes_specials_and_encodes_non_ascii() {
    assert_eq!(phrase("Alice Liddell"), "Alice Liddell");
    assert_eq!(phrase("Liddell, Alice"), "\"Liddell, Alice\"");
    assert_eq!(phrase("Al \"the\" Great"), "\"Al \\\"the\\\" Great\"");
    assert_eq!(phrase("back\\slash"), "\"back\\\\slash\"");
    assert_eq!(phrase("Dr. Who"), "\"Dr. Who\"");
    let encoded = phrase("Zoë");
    assert!(encoded.starts_with("=?UTF-8?B?") && is_plain_header_text(&encoded));
}

#[test]
fn short_headers_are_not_folded() {
    assert_eq!(
        fold_header("Subject", "Hello there"),
        "Subject: Hello there"
    );
}

#[test]
fn long_headers_fold_at_spaces_within_78_columns() {
    let value = "word ".repeat(40);
    let folded = fold_header("Subject", value.trim());
    for line in folded.split("\r\n") {
        assert!(line.len() <= 78, "{line:?} is {}", line.len());
    }
    assert!(folded.contains("\r\n "), "expected at least one fold");
    // Unfolding gives back the original.
    let unfolded = folded.replace("\r\n ", " ");
    assert_eq!(unfolded, format!("Subject: {}", value.trim()));
}

#[test]
fn folding_never_splits_a_token_longer_than_a_line() {
    let long = "x".repeat(200);
    let folded = fold_header("X-Long", &long);
    assert_eq!(folded, format!("X-Long: {long}"));
    let folded = fold_header("X-Long", &format!("short {long} tail"));
    assert!(folded.contains(&long));
}

#[test]
fn seven_bit_detection() {
    assert!(is_7bit("plain\r\ntext"));
    assert!(!is_7bit("caf\u{e9}"));
    assert!(!is_7bit("nul\0byte"));
    assert!(!is_7bit(&"x".repeat(999)));
    assert!(is_7bit(&"x".repeat(998)));
    assert!(is_7bit(&format!(
        "{}\r\n{}",
        "x".repeat(998),
        "y".repeat(998)
    )));
}

#[test]
fn quoted_printable_known_vectors() {
    assert_eq!(quoted_printable("hello"), "hello");
    assert_eq!(quoted_printable("caf\u{e9}"), "caf=C3=A9");
    assert_eq!(quoted_printable("a=b"), "a=3Db");
    assert_eq!(quoted_printable("tab\there"), "tab\there");
    assert_eq!(quoted_printable("two\r\nlines"), "two\r\nlines");
    assert_eq!(quoted_printable("ends with crlf\r\n"), "ends with crlf\r\n");
    assert_eq!(quoted_printable(""), "");
}

#[test]
fn quoted_printable_protects_trailing_whitespace() {
    assert_eq!(quoted_printable("trailing "), "trailing=20");
    assert_eq!(quoted_printable("trailing\t"), "trailing=09");
    assert_eq!(quoted_printable("a \r\nb"), "a=20\r\nb");
}

#[test]
fn quoted_printable_lines_never_exceed_76_and_round_trip() {
    for text in [
        "x".repeat(500),
        "é".repeat(120),
        format!("{}=", "y".repeat(74)),
        "word ".repeat(80),
        format!("{}\r\nshort\r\n{}", "a".repeat(300), "\u{1f600}".repeat(40)),
        "=".repeat(60),
    ] {
        let encoded = quoted_printable(&text);
        for line in encoded.split("\r\n") {
            assert!(line.len() <= 76, "{} chars: {line:?}", line.len());
        }
        assert!(!encoded.contains("\r\n\r\n") || text.contains("\r\n\r\n"));
        // No soft break dangling at the very end of the text.
        assert!(!encoded.ends_with("=\r\n") || text.ends_with("\r\n"));
        assert_eq!(qp_decode(&encoded), text.as_bytes(), "round trip");
    }
}

#[test]
fn a_line_of_exactly_76_characters_needs_no_soft_break() {
    let text = "a".repeat(76);
    assert_eq!(quoted_printable(&text), text);
    let text = "a".repeat(77);
    let encoded = quoted_printable(&text);
    assert_eq!(qp_decode(&encoded), text.as_bytes());
    assert!(encoded.contains("=\r\n"));
}
