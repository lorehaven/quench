//! Expansion tests: every shape both derives handle, and every way they refuse.

use proc_macro2::TokenStream;

use super::{expand_deserialize, expand_serialize};

fn squash(tokens: &TokenStream) -> String {
    tokens
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn ser(source: &str) -> String {
    squash(&expand_serialize(source.parse().unwrap()))
}

fn de(source: &str) -> String {
    squash(&expand_deserialize(source.parse().unwrap()))
}

fn ok(out: &str) {
    assert!(!out.contains("compile_error"), "{out}");
    assert!(out.contains("impl"), "{out}");
}

fn err(out: &str, needle: &str) {
    assert!(
        out.contains("compile_error") && out.contains(needle),
        "wanted {needle:?}: {out}"
    );
}

const SHAPES: [&str; 14] = [
    "struct S { a: u8, b: String }",
    "struct S;",
    "struct S(u8);",
    "struct S(u8, String);",
    "struct S<T> { v: T }",
    "struct S<'a> { v: &'a str }",
    "enum E { A, B(u8), C(u8, u8), D { x: u8 } }",
    "enum E {}",
    r#"#[serde(tag = "t")] enum E { A, B(S), C { x: u8 } }"#,
    "#[serde(untagged)] enum E { A, B(u8), C(u8, u8), D { x: u8 } }",
    r#"#[serde(rename_all = "kebab-case")] enum E { FirstOne, #[serde(rename_all = "camelCase")] Second { inner_name: u8 } }"#,
    "#[serde(default, deny_unknown_fields)] struct S { a: u8 }",
    "struct S { #[serde(flatten)] rest: M, #[serde(flatten)] more: N }",
    r#"struct S { #[serde(default = "f", rename = "x", alias = "y", skip_serializing_if = "g")] a: u8, #[serde(skip)] b: u8 }"#,
];

#[test]
fn every_shape_expands_for_both_derives() {
    for shape in SHAPES {
        ok(&ser(shape));
        ok(&de(shape));
    }
}

#[test]
fn serialize_output_shapes() {
    let out = ser("struct S { a: u8, #[serde(rename = \"b2\")] b: u8 }");
    assert!(
        out.contains(
            "__map . insert (\"a\" , :: quench_serde :: Serialize :: serialize (& self . a))"
        ),
        "{out}"
    );
    assert!(out.contains("insert (\"b2\""), "{out}");
    assert!(ser("struct S(u8);").contains("Serialize :: serialize (& self . 0)"));
    assert!(ser("struct S(u8, u8);").contains("Value :: Array"));
    assert!(ser("struct S;").contains("Value :: Null"));
    assert!(ser("enum E {}").contains("match * self { }"));
    let skip = ser(
        r#"struct S { #[serde(skip_serializing_if = "Option::is_none")] a: Option<u8>, #[serde(skip)] b: u8, #[serde(flatten)] c: M }"#,
    );
    assert!(
        skip.contains("if ! Option :: is_none (& self . a)"),
        "{skip}"
    );
    assert!(!skip.contains("self . b"), "{skip}");
    assert!(skip.contains("flatten_into"), "{skip}");
    let external = ser("enum E { Unit, New(u8), Many(u8, u8), Named { a: u8 } }");
    assert!(
        external.contains("Self :: Unit => :: quench_serde :: Value :: String"),
        "{external}"
    );
    assert!(external.contains("Self :: New (__f0) =>"), "{external}");
    assert!(external.contains("Self :: Named { a } =>"), "{external}");
    let tagged = ser(r#"#[serde(tag = "kind")] enum E { U, N(S), S { a: u8 } }"#);
    assert!(tagged.contains("insert (\"kind\""), "{tagged}");
    let untagged = ser("#[serde(untagged)] enum E { U, N(u8) }");
    assert!(
        untagged.contains("Self :: U => :: quench_serde :: Value :: Null"),
        "{untagged}"
    );
}

#[test]
fn generics_get_bounds() {
    assert!(ser("struct S<T> { v: T }").contains("where T : :: quench_serde :: Serialize"));
    assert!(
        de("struct S<T, U> { v: T, w: U }")
            .contains("T : :: quench_serde :: Deserialize , U : :: quench_serde :: Deserialize")
    );
    // lifetimes and consts get none
    assert!(!ser("struct S<'a> { v: &'a str }").contains("where"));
}

#[test]
fn deserialize_output_shapes() {
    let named =
        de(r#"#[serde(deny_unknown_fields)] struct S { a: u8, #[serde(alias = "bee")] b: u8 }"#);
    assert!(
        named.contains("check_unknown (__object , & [\"a\" , \"b\" , \"bee\"])"),
        "{named}"
    );
    let flat = de("#[serde(deny_unknown_fields)] struct S { a: u8, #[serde(flatten)] rest: M }");
    assert!(flat.contains("let __rest"), "{flat}");
    assert!(!flat.contains("check_unknown"), "{flat}");
    let defaults = de(
        r#"struct S { #[serde(default)] a: u8, #[serde(default = "f")] b: u8, #[serde(skip)] c: u8 }"#,
    );
    assert!(
        defaults
            .contains("field_or (__object , & [\"a\"] , :: core :: default :: Default :: default)"),
        "{defaults}"
    );
    assert!(
        defaults.contains("field_or (__object , & [\"b\"] , f)"),
        "{defaults}"
    );
    assert!(
        defaults.contains("c : :: core :: default :: Default :: default ()"),
        "{defaults}"
    );
    let container = de("#[serde(default)] struct S { a: u8, #[serde(skip)] b: u8 }");
    assert!(
        container.contains("let __default : Self = :: core :: default :: Default :: default ()"),
        "{container}"
    );
    assert!(container.contains("__default . a"), "{container}");
    assert!(container.contains("b : __default . b"), "{container}");
    let by_path = de(r#"#[serde(default = "make")] struct S { a: u8 }"#);
    assert!(
        by_path.contains("let __default : Self = make ()"),
        "{by_path}"
    );
    assert!(de("struct S(u8);").contains("Deserialize :: deserialize (__value)"));
    assert!(
        de("struct S(u8, u8);").contains("expect_seq (__value , 2usize)"),
        "{}",
        de("struct S(u8, u8);")
    );
    assert!(de("struct S;").contains("expect_null"));
    assert!(de("enum E {}").contains("uninhabited"));
}

#[test]
fn enum_representations_deserialize() {
    let external = de("enum E { A, B(u8), C { x: u8 } }");
    assert!(external.contains("single_entry"), "{external}");
    assert!(external.contains("unknown_variant"), "{external}");
    assert!(external.contains("in_field (\"B\")"), "{external}");
    let tagged = de(r#"#[serde(tag = "t")] enum E { A, B(S), C { x: u8 } }"#);
    assert!(tagged.contains("tag_of (__object , \"t\")"), "{tagged}");
    assert!(tagged.contains("rest (__object , & [\"t\"])"), "{tagged}");
    let untagged = de("#[serde(untagged)] enum E { A, B(u8) }");
    assert!(
        untagged.contains("did not match any variant of untagged enum"),
        "{untagged}"
    );
    let aliases = de(r#"enum E { #[serde(alias = "old", rename = "new")] A }"#);
    assert!(aliases.contains("\"new\" | \"old\""), "{aliases}");
}

#[test]
fn rename_rules_apply_to_keys() {
    let out = ser(r#"#[serde(rename_all = "SCREAMING_SNAKE_CASE")] struct S { first_name: u8 }"#);
    assert!(out.contains("insert (\"FIRST_NAME\""), "{out}");
    let raw = ser("struct S { r#type: u8 }");
    assert!(raw.contains("insert (\"type\""), "{raw}");
    let variants = ser(r#"#[serde(rename_all = "snake_case")] enum E { FirstOne }"#);
    assert!(variants.contains("\"first_one\""), "{variants}");
}

#[test]
fn refusals() {
    err(&ser("union U { a: u8 }"), "unions are not supported");
    err(&de("union U { a: u8 }"), "unions are not supported");
    err(
        &ser("struct S(#[serde(flatten)] u8);"),
        "`flatten` needs a named field",
    );
    err(
        &ser(r#"#[serde(tag = "t")] enum E { A(u8, u8) }"#),
        "cannot be internally tagged",
    );
    err(
        &ser(r#"#[serde(tag = "t", untagged)] enum E { A }"#),
        "cannot be combined",
    );
    err(
        &ser("#[serde(bogus)] struct S;"),
        "unsupported serde container attribute",
    );
    err(
        &de("struct S { #[serde(with = \"m\")] a: u8 }"),
        "unsupported serde field attribute",
    );
    err(
        &de("enum E { #[serde(skip)] A }"),
        "unsupported serde variant attribute",
    );
    err(&ser("not rust at all"), "compile_error");
    err(&de(""), "compile_error");
}
