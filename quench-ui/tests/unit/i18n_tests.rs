//! Runtime translations: Fluent subset, plural rules (English and Polish), language switching.

use quench_ui::i18n::{Catalog, Category, I18n, Value, plural_category};
use quench_ui::prelude::*;
use std::rc::Rc;

const EN: &str = "\
# a comment
## a group
hello = Hello, { $name }!
-brand = Quench
about = About { -brand }
files = { $n ->
    [0] No files
    [one] { $n } file
   *[other] { $n } files
}
nested = You have { files }
literal = { \"quoted\" } and { NUMBER($n) }
multi = first
    second
    .title = ignored attribute
title = Notes
";

const PL: &str = "\
hello = Cześć, { $name }!
files = { $n ->
    [one] { $n } plik
    [few] { $n } pliki
    [many] { $n } plików
   *[other] { $n } pliku
}
";

fn en() -> Catalog {
    Catalog::parse(EN).unwrap()
}

#[quench_test::test]
fn variables_references_and_literals_are_substituted() {
    let c = en();
    assert_eq!(
        c.format("en", "hello", &[("name", "Ola".into())])
            .as_deref(),
        Some("Hello, Ola!")
    );
    assert_eq!(
        c.format("en", "hello", &[]).as_deref(),
        Some("Hello, !"),
        "a missing argument is empty"
    );
    assert_eq!(
        c.format("en", "about", &[]).as_deref(),
        Some("About "),
        "terms contribute nothing"
    );
    assert_eq!(
        c.format("en", "nested", &[("n", 3.into())]).as_deref(),
        Some("You have 3 files")
    );
    assert_eq!(
        c.format("en", "literal", &[("n", 4.5.into())]).as_deref(),
        Some("quoted and 4.5")
    );
    assert_eq!(
        c.format("en", "multi", &[]).as_deref(),
        Some("first\nsecond"),
        "attributes are dropped"
    );
    assert_eq!(c.format("en", "title", &[]).as_deref(), Some("Notes"));
    assert_eq!(c.format("en", "nope", &[]), None);
    assert!(c.contains("hello") && !c.contains("-brand") && !c.contains("nope"));
}

#[quench_test::test]
fn selectors_pick_an_exact_number_then_the_plural_category() {
    let c = en();
    let files = |n: i32| c.format("en", "files", &[("n", n.into())]).unwrap();
    assert_eq!(files(0), "No files");
    assert_eq!(files(1), "1 file");
    assert_eq!(files(2), "2 files");
    assert_eq!(
        c.format("en", "files", &[("n", 1.0_f64.into())]).as_deref(),
        Some("1 file"),
        "1.0 prints as 1: an integer"
    );
    assert_eq!(
        c.format("en", "files", &[("n", 1.5.into())]).as_deref(),
        Some("1.5 files")
    );
    assert_eq!(
        c.format("en", "files", &[]).as_deref(),
        Some(" files"),
        "no value: the default"
    );
    let words =
        Catalog::parse("kind = { $k ->\n    [cat] a cat\n   *[other] something\n}").unwrap();
    assert_eq!(
        words
            .format("en", "kind", &[("k", "cat".into())])
            .as_deref(),
        Some("a cat")
    );
    assert_eq!(
        words
            .format("en", "kind", &[("k", "dog".into())])
            .as_deref(),
        Some("something")
    );
}

#[quench_test::test]
fn polish_has_four_plural_forms() {
    let pl = Catalog::parse(PL).unwrap();
    let files = |n: i32| pl.format("pl", "files", &[("n", n.into())]).unwrap();
    for (n, want) in [
        (1, "1 plik"),
        (2, "2 pliki"),
        (3, "3 pliki"),
        (4, "4 pliki"),
        (5, "5 plików"),
        (0, "0 plików"),
        (11, "11 plików"),
        (12, "12 plików"),
        (13, "13 plików"),
        (14, "14 plików"),
        (21, "21 plików"),
        (22, "22 pliki"),
        (24, "24 pliki"),
        (25, "25 plików"),
        (100, "100 plików"),
        (101, "101 plików"),
        (102, "102 pliki"),
        (112, "112 plików"),
        (1012, "1012 plików"),
    ] {
        assert_eq!(files(n), want, "{n}");
    }
    assert_eq!(
        pl.format("pl", "files", &[("n", 1.5.into())]).as_deref(),
        Some("1.5 pliku"),
        "fractions are `other`"
    );
    assert_eq!(plural_category("pl-PL", 3.0), Category::Few);
}

#[quench_test::test]
fn other_languages_follow_their_rules_or_english() {
    assert_eq!(plural_category("en", 1.0), Category::One);
    assert_eq!(plural_category("en", 0.0), Category::Other);
    assert_eq!(plural_category("en_US", 1.0), Category::One);
    assert_eq!(plural_category("ja", 1.0), Category::Other);
    assert_eq!(plural_category("fr", 0.0), Category::One);
    assert_eq!(plural_category("fr", 1.5), Category::Other);
    assert_eq!(plural_category("xx", 1.0), Category::One);
    assert_eq!(plural_category("en", -1.0), Category::One);
}

#[quench_test::test]
fn bad_files_are_reported_with_a_line() {
    for (text, why) in [
        ("a = { $n", "unclosed"),
        ("a = { $n -> [one] x }", "default"),
        ("a = }", "unbalanced"),
        ("a = { $n -> x }", "variant"),
    ] {
        let e = Catalog::parse(text).unwrap_err();
        assert_eq!(e.line, 1, "{text}");
        assert!(e.to_string().contains("line 1"), "{why}: {e}");
    }
    assert!(
        Catalog::parse("# only a comment\n\n")
            .unwrap()
            .format("en", "x", &[])
            .is_none()
    );
    assert!(
        Catalog::parse("a =\n")
            .unwrap()
            .format("en", "a", &[])
            .is_none(),
        "no value, no message"
    );
    let e = I18n::new(&[("en", "a = {")], "en").unwrap_err();
    assert!(e.message.starts_with("en:"));
}

#[quench_test::test]
fn switching_the_language_updates_what_reads_it() {
    let i18n = I18n::new(&[("en", EN), ("pl", PL)], "en").unwrap();
    assert_eq!(i18n.languages(), ["en", "pl"]);
    let (r, count) = (Rc::new(TestRenderer::new()), create_signal(1));
    let (view_i18n, view_count) = (i18n.clone(), count.clone());
    let m = mount(
        &r,
        &r.root(),
        span().text_fn(move || view_i18n.tr("files", &[("n", view_count.get().into())])),
    );
    assert_eq!(r.text(), "1 file");
    count.set(2);
    assert_eq!(r.text(), "2 files");
    i18n.set_language("pl");
    assert_eq!(r.text(), "2 pliki");
    count.set(5);
    assert_eq!(r.text(), "5 plików");
    i18n.set_language("pl-PL");
    assert_eq!(r.text(), "5 plików", "a regional code finds the language");
    assert_eq!(i18n.language(), "pl-PL");
    // a message only the first language has falls back to it; an unknown id is shown as itself
    assert_eq!(i18n.t("title"), "Notes");
    assert_eq!(i18n.t("no-such-message"), "no-such-message");
    assert_eq!(
        i18n.tr("hello", &[("name", Value::from("Ewa".to_string()))]),
        "Cześć, Ewa!"
    );
    assert!(format!("{i18n:?}").contains("pl-PL"));
    m.unmount();
}
