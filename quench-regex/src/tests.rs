use super::*;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|e| panic!("{pattern}: {e}"))
}

fn spans(pattern: &str, text: &str) -> Vec<(usize, usize)> {
    re(pattern)
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect()
}

fn error(pattern: &str) -> Error {
    Regex::new(pattern).expect_err(pattern)
}

#[test]
fn literals_classes_and_dots() {
    assert_eq!(spans("abc", "xabcabc"), [(1, 4), (4, 7)]);
    assert_eq!(spans("a.c", "abc a\nc"), [(0, 3)]);
    assert_eq!(spans("(?s)a.c", "a\nc"), [(0, 3)]);
    assert_eq!(spans("[a-c]+", "xabcdcba"), [(1, 4), (5, 8)]);
    assert_eq!(spans("[^a-c]+", "abxyc"), [(2, 4)]);
    assert_eq!(spans("[]a]+", "x]a]"), [(1, 4)]);
    assert_eq!(spans("[a-]+", "-a-b"), [(0, 3)]);
    assert_eq!(spans(r"[\d_]+", "a1_2b"), [(1, 4)]);
    assert_eq!(spans(r"[[:alpha:]]+", "12ab34"), [(2, 4)]);
    assert_eq!(spans(r"[[:^alpha:]]+", "12ab34"), [(0, 2), (4, 6)]);
    assert_eq!(spans(r"[[a-b][x-z]]+", "axc"), [(0, 2)]);
    assert_eq!(spans(r"\.", "a.b"), [(1, 2)]);
    assert_eq!(spans(r"\x41\u{e9}B", "Aéb AéB"), [(5, 9)]);
}

#[test]
fn unicode_classes_and_boundaries() {
    assert_eq!(spans(r"\w+", "zażółć gęślą"), [(0, 10), (11, 19)]);
    assert_eq!(spans(r"\d+", "٣٤ 56"), [(0, 4), (5, 7)]);
    assert_eq!(spans(r"\s", "a\u{a0}b c"), [(1, 3), (4, 5)]);
    assert_eq!(spans(r"\bfoo\b", "foo food foo"), [(0, 3), (9, 12)]);
    assert_eq!(spans(r"\Boo\B", "foo food"), [(5, 7)]);
    assert_eq!(spans(r"\W+", "ab, cd"), [(2, 4)]);
    assert_eq!(spans(r"\D+", "ab12"), [(0, 2)]);
    assert_eq!(spans(r"\S+", "ab cd"), [(0, 2), (3, 5)]);
    assert_eq!(spans(r"[^\W\d]+", "ab1c"), [(0, 2), (3, 4)]);
}

#[test]
fn case_insensitivity_follows_unicode() {
    assert_eq!(spans("(?i)straße", "STRASSE Straße"), [(8, 15)]);
    assert!(re("(?i)k").is_match("\u{212A}"), "the Kelvin sign");
    assert!(re("(?i)σ").is_match("ς") && re("(?i)σ").is_match("Σ"));
    assert!(re("(?i)[a-c]").is_match("B"));
    assert!(re("(?i)[^a-c]").is_match("D") && !re("(?i)[^a-c]").is_match("B"));
    assert!(!re("k").is_match("K"));
    assert!(re("(?i:k)x").is_match("Kx") && !re("(?i:k)x").is_match("KX"));
    let mut builder = RegexBuilder::new("abc");
    assert!(
        builder
            .case_insensitive(true)
            .build()
            .unwrap()
            .is_match("ABC")
    );
}

#[test]
fn anchors_and_multiline() {
    assert_eq!(spans("^a", "a\na"), [(0, 1)]);
    assert_eq!(spans("(?m)^a", "a\na"), [(0, 1), (2, 3)]);
    assert_eq!(spans("a$", "a\na"), [(2, 3)]);
    assert_eq!(spans("(?m)a$", "a\na"), [(0, 1), (2, 3)]);
    assert_eq!(spans(r"\Aa", "aa"), [(0, 1)]);
    assert_eq!(spans(r"a\z", "aa"), [(1, 2)]);
    assert!(re("^$").is_match(""));
    assert!(!re("^$").is_match("\n"));
    assert!(
        re(r"^").find_at("ab", 1).is_none(),
        "assertions see the whole text"
    );
}

#[test]
fn repetition_greedy_and_lazy() {
    assert_eq!(spans("a*", "baaac"), [(0, 0), (1, 4), (5, 5)]);
    assert_eq!(spans("a+?", "aaa"), [(0, 1), (1, 2), (2, 3)]);
    assert_eq!(spans("a{2}", "aaaaa"), [(0, 2), (2, 4)]);
    assert_eq!(spans("a{2,3}", "aaaaaaa"), [(0, 3), (3, 6)]);
    assert_eq!(spans("a{2,}", "aaaa b"), [(0, 4)]);
    assert_eq!(spans("a{0,2}?b", "aab"), [(0, 3)]);
    assert_eq!(spans("<.+>", "<a><b>"), [(0, 6)]);
    assert_eq!(spans("<.+?>", "<a><b>"), [(0, 3), (3, 6)]);
    assert_eq!(spans("colou?r", "color colour"), [(0, 5), (6, 12)]);
    assert_eq!(
        spans("(a|ab)(c|bcd)", "abcd"),
        [(0, 4)],
        "leftmost-first, not longest"
    );
    assert_eq!(spans("(?U)a+", "aaa"), [(0, 1), (1, 2), (2, 3)]);
    assert_eq!(spans("(?U)a+?", "aaa"), [(0, 3)]);
    assert_eq!(
        RegexBuilder::new("a+")
            .swap_greed(true)
            .build()
            .unwrap()
            .find("aaa")
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn groups_and_captures() {
    let r = re(r"(?P<key>\w+)=(?<value>\d+)(?:;|$)");
    assert_eq!(r.captures_len(), 3);
    assert_eq!(
        r.capture_names().collect::<Vec<_>>(),
        [None, Some("key"), Some("value")]
    );
    let caps = r.captures("a=1;bb=22").unwrap();
    assert_eq!((&caps[0], &caps[1], &caps["value"]), ("a=1;", "a", "1"));
    assert_eq!(caps.len(), 3);
    assert!(!caps.is_empty());
    assert_eq!(caps.get(2).map(|m| m.range()), Some(2..3));
    assert_eq!(caps.name("key").map(|m| m.as_str()), Some("a"));
    assert!(caps.name("nope").is_none() && caps.get(9).is_none());
    assert_eq!(caps.iter().flatten().count(), 3);
    assert!(format!("{caps:?}").contains("Match"));

    let all: Vec<String> = r
        .captures_iter("a=1;bb=22")
        .map(|c| c["key"].to_string())
        .collect();
    assert_eq!(all, ["a", "bb"]);

    let optional = re(r"(a)|(b)").captures("b").unwrap();
    assert!(optional.get(1).is_none() && optional.get(2).is_some());
    assert!(
        std::panic::catch_unwind(|| {
            let c = re(r"(a)|(b)").captures("b").unwrap();
            let _ = &c[1];
        })
        .is_err()
    );
    assert!(
        std::panic::catch_unwind(|| {
            let c = re("a").captures("a").unwrap();
            let _ = &c["missing"];
        })
        .is_err()
    );
}

#[test]
fn matches_and_their_conversions() {
    let m = re("b+").find("abbbc").unwrap();
    assert_eq!((m.start(), m.end(), m.len(), m.range()), (1, 4, 3, 1..4));
    assert!(!m.is_empty());
    assert_eq!(m.as_str(), "bbb");
    let s: &str = m.into();
    assert_eq!(s, "bbb");
    let range: Range<usize> = m.into();
    assert_eq!(range, 1..4);
    assert!(format!("{m:?}").contains("bbb"));
    assert!(re("x*").find("").unwrap().is_empty());
    assert!(re("b").find("aaa").is_none());
    assert_eq!(re("b").find_at("abab", 2).map(|m| m.start()), Some(3));
    assert_eq!(
        re("(b)")
            .captures_at("abab", 2)
            .map(|c| c.get(0).unwrap().start()),
        Some(3)
    );
}

#[test]
fn replacement_forms() {
    let r = re(r"(?P<a>\w)(\d)");
    assert_eq!(r.replace("x1 y2", "<$a-$2>"), "<x-1> y2");
    assert_eq!(r.replace_all("x1 y2", "<${a}${2}>"), "<x1> <y2>");
    assert_eq!(r.replace_all("x1", "$$ $0 $9 $nope $"), "$ x1   $");
    assert_eq!(r.replace_all("x1", "${"), "${");
    assert_eq!(r.replace_all("x1", NoExpand("$a")), "$a");
    assert_eq!(r.replacen("x1 y2 z3", 2, "_"), "_ _ z3");
    assert_eq!(
        r.replace_all("x1 y2", |c: &Captures<'_>| c[1].to_uppercase()),
        "X Y"
    );
    assert_eq!(r.replace_all("x1", String::from("[$1]")), "[x]");
    assert_eq!(r.replace_all("x1", &String::from("[$1]")), "[x]");
    assert_eq!(r.replace_all("x1", Cow::Borrowed("[$1]")), "[x]");
    assert_eq!(r.replace_all("x1", Cow::<str>::Owned("$$".into())), "$");
    assert_eq!(r.replace_all("x1", String::from("plain")), "plain");
    assert_eq!(r.replace_all("x1", &String::from("plain")), "plain");
    assert_eq!(r.replace_all("x1", "plain"), "plain");
    assert!(matches!(
        r.replace_all("nothing here", "x"),
        Cow::Borrowed(_)
    ));
    assert!(matches!(
        r.replace_all("nothing here", |_: &Captures<'_>| "x"),
        Cow::Borrowed(_)
    ));
    assert_eq!(re("a*").replace_all("baac", "-"), "-b-c-");
    assert_eq!(re("é").replace_all("café", "e"), "cafe");
}

#[test]
fn splitting() {
    assert_eq!(
        re(r"\s*,\s*").split("a , b,c").collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(
        re(",").split("a,,b,").collect::<Vec<_>>(),
        ["a", "", "b", ""]
    );
    assert_eq!(re(",").split("none").collect::<Vec<_>>(), ["none"]);
    assert_eq!(re("").split("ab").collect::<Vec<_>>(), ["", "a", "b", ""]);
    assert!(format!("{:?}", re(",").split("a")).contains("Split"));
}

#[test]
fn empty_matches_do_not_stick_to_the_previous_match() {
    assert_eq!(spans("", "ab"), [(0, 0), (1, 1), (2, 2)]);
    assert_eq!(spans("", "é"), [(0, 0), (2, 2)]);
    assert_eq!(spans("a|", "ba"), [(0, 0), (1, 2)]);
    assert!(format!("{:?}", re("a").find_iter("a")).contains("Matches"));
    assert!(format!("{:?}", re("a").captures_iter("a")).contains("CaptureMatches"));
}

#[test]
fn alternation_and_nesting() {
    assert_eq!(spans("cat|dog", "hotdog catalog"), [(3, 6), (7, 10)]);
    assert_eq!(spans("a(b|c)*d", "abcbd ad"), [(0, 5), (6, 8)]);
    assert_eq!(spans("(?:ab)+", "ababab"), [(0, 6)]);
    assert_eq!(spans("((a)(b))", "ab"), [(0, 2)]);
    assert_eq!(spans("(?i)a(?-i)b", "Ab AB"), [(0, 2)]);
    assert_eq!(spans("(?i:a)b", "Ab AB"), [(0, 2)]);
    assert_eq!(spans("(?s:.)", "\n"), [(0, 1)]);
    assert_eq!(spans("(?is)A.B", "a\nb"), [(0, 3)]);
}

#[test]
fn verbose_mode_ignores_layout() {
    assert_eq!(spans("(?x) a b  c # a comment\n d", "abcd"), [(0, 4)]);
    assert_eq!(spans(r"(?x) \  ", "a b"), [(1, 2)]);
    assert_eq!(
        spans("(?x) a{1,2} ?", "aaa"),
        [(0, 1), (1, 2), (2, 3)],
        "a counted repeat can be made lazy across a space"
    );
    let mut builder = RegexBuilder::new("a b # c");
    assert!(
        builder
            .ignore_whitespace(true)
            .build()
            .unwrap()
            .is_match("ab")
    );
}

#[test]
fn builder_flags() {
    assert!(
        RegexBuilder::new("^a$")
            .multi_line(true)
            .build()
            .unwrap()
            .is_match("x\na\ny")
    );
    assert!(
        RegexBuilder::new("a.b")
            .dot_matches_new_line(true)
            .build()
            .unwrap()
            .is_match("a\nb")
    );
    assert!(!RegexBuilder::new("a.b").build().unwrap().is_match("a\nb"));
    let regex: Regex = "a+".parse().unwrap();
    assert_eq!(regex.as_str(), "a+");
    assert_eq!(regex.to_string(), "a+");
    assert_eq!(format!("{regex:?}"), "Regex(\"a+\")");
    assert!(format!("{:?}", RegexBuilder::new("x")).contains("RegexBuilder"));
}

#[test]
fn escaping() {
    assert_eq!(escape("a.b*c"), r"a\.b\*c");
    assert_eq!(escape("(1+1)=[2]"), r"\(1\+1\)=\[2\]");
    assert_eq!(escape("é_"), "é_");
    let tricky = "1+1=2? [yes] {no} $5 ^up | a\\b #x &y -z ~";
    assert!(re(&escape(tricky)).is_match(tricky));
    assert_eq!(
        re(&escape(tricky)).find(tricky).map(|m| m.as_str().len()),
        Some(tricky.len())
    );
}

#[test]
fn bad_patterns_are_rejected_with_a_place() {
    let cases = [
        ("(", "unclosed group"),
        (")", "unopened group"),
        ("a)", "unopened group"),
        ("[a", "unclosed character class"),
        ("[b-a]", "out of order"),
        ("[a-\\d]", "shorthand cannot end a range"),
        ("*", "missing"),
        ("+a", "missing"),
        ("{2}", "missing"),
        ("a{2", "unclosed counted repetition"),
        ("a{x}", "decimal number"),
        ("a{2,x}", "decimal number"),
        ("a{3,2}", "minimum above its maximum"),
        ("a{1001}", "too large"),
        ("a{1,1001}", "too large"),
        (r"\y", "unrecognized escape"),
        (r"\", "lone"),
        (r"\xZ", "hexadecimal"),
        (r"\x{110000}", "hexadecimal"),
        (r"\x{41", "hexadecimal"),
        (r"\u12", "hexadecimal"),
        ("(?P<>a)", "capture group name"),
        ("(?P<1a>a)", "capture group name"),
        ("(?P<a b>a)", "capture group name"),
        ("(?Pa)", "expected `<`"),
        ("(?P<a>x)(?P<a>y)", "duplicate"),
        ("(?q)", "unrecognized flag"),
        ("(?i", "unclosed group"),
        ("[[:nope:]]", "unknown POSIX class"),
        ("[[:alpha]", "malformed POSIX class"),
        ("[a&&b]", "set operations"),
        ("[a--b]", "out of order"),
        ("[a~~b]", "set operations"),
    ];
    for (pattern, wanted) in cases {
        let e = error(pattern);
        assert!(e.message().contains(wanted), "{pattern}: {e}");
        assert!(e.to_string().contains("regex parse error"), "{e}");
        assert!(e.offset().is_some());
    }
    let deep = format!("{}a{}", "(".repeat(300), ")".repeat(300));
    assert!(error(&deep).message().contains("nested too deeply"));
    let big = error("((a{1000}){1000}){1000}");
    assert!(big.message().contains("too large"));
    assert_eq!(big.offset(), None);
    assert!(big.to_string().starts_with("regex error"));
    assert!(std::error::Error::source(&big).is_none());
}

#[test]
fn patterns_that_could_blow_up_other_engines_stay_fast() {
    let r = re("(a+)+$");
    let text = format!("{}b", "a".repeat(5000));
    assert!(!r.is_match(&text));
    let nested = re("(a|aa)*b");
    assert!(!nested.is_match(&"a".repeat(5000)));
    // Large counted repeats are fine as long as the program stays small enough.
    assert!(re("a{1000}").is_match(&"a".repeat(1000)));
}

#[test]
fn literal_prefixes_and_anchors_are_only_shortcuts() {
    assert_eq!(spans("abc[0-9]", "xabcx abc1 abc2"), [(6, 10), (11, 15)]);
    assert_eq!(spans("é+", "aééb"), [(1, 5)]);
    assert!(re("abc").find_at("abcabc", 4).is_none());
    assert_eq!(spans("^ab", "abab"), [(0, 2)]);
    assert!(re("^(a|b)").find_at("ba", 1).is_none());
    assert_eq!(spans("(?:^a|^b)", "ba"), [(0, 1)]);
}

#[test]
fn repeating_a_repeat_is_allowed() {
    assert_eq!(spans("a**", "aab"), [(0, 2), (3, 3)]);
}

#[test]
fn the_class_helpers_agree_with_the_tables() {
    assert!(class::is_word('ł') && class::is_word('_') && !class::is_word(' '));
    assert!(class::word().contains('ł'));
}
