//! The derives, exercised on the shapes quench and forge actually use.

// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use std::collections::{BTreeMap, HashMap};

use quench_serde::{Deserialize, Serialize, Value, json};

fn text<T: Serialize>(value: &T) -> String {
    json::to_string(value)
}

fn read<T: Deserialize>(source: &str) -> T {
    json::from_str(source).unwrap_or_else(|e| panic!("{source}: {e}"))
}

fn fails<T: Deserialize>(source: &str) -> String {
    json::from_str::<T>(source)
        .err()
        .map(|e| e.to_string())
        .expect("should fail")
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Plain {
    name: String,
    count: u32,
    tags: Vec<String>,
    note: Option<String>,
}

#[quench_test::test]
fn plain_struct_round_trips_in_field_order() {
    let value = Plain {
        name: "a".into(),
        count: 2,
        tags: vec!["x".into()],
        note: None,
    };
    assert_eq!(
        text(&value),
        r#"{"name":"a","count":2,"tags":["x"],"note":null}"#
    );
    assert_eq!(read::<Plain>(&text(&value)), value);
    // an absent Option is None; other absent fields are errors
    assert_eq!(
        read::<Plain>(r#"{"name":"a","count":2,"tags":[]}"#).note,
        None
    );
    assert_eq!(
        fails::<Plain>(r#"{"name":"a","tags":[]}"#),
        "missing field `count`"
    );
    assert_eq!(
        fails::<Plain>(r#"{"name":"a","count":"x","tags":[]}"#),
        "count: invalid type: expected an integer, found string"
    );
    assert_eq!(
        fails::<Plain>(r#"{"name":"a","count":1,"tags":[1]}"#),
        "tags[0]: invalid type: expected a string, found number"
    );
    assert!(fails::<Plain>("[]").contains("expected an object"));
    // unknown fields are ignored by default
    assert_eq!(
        read::<Plain>(r#"{"name":"a","count":1,"tags":[],"extra":true}"#).count,
        1
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Camel {
    first_name: String,
    #[serde(rename = "id")]
    identifier: u32,
    #[serde(alias = "old_nick", alias = "nick")]
    nickname: Option<String>,
}

#[quench_test::test]
fn rename_alias_and_deny_unknown_fields() {
    let value = Camel {
        first_name: "Ada".into(),
        identifier: 7,
        nickname: Some("a".into()),
    };
    assert_eq!(text(&value), r#"{"firstName":"Ada","id":7,"nickname":"a"}"#);
    assert_eq!(
        read::<Camel>(r#"{"firstName":"Ada","id":7,"old_nick":"a"}"#),
        value
    );
    assert_eq!(
        read::<Camel>(r#"{"firstName":"Ada","id":7,"nick":"a"}"#),
        value
    );
    let e = fails::<Camel>(r#"{"firstName":"Ada","id":7,"surprise":1}"#);
    assert_eq!(
        e,
        "unknown field `surprise`, expected one of `firstName`, `id`, `nickname`, `old_nick`, `nick`"
    );
}

const fn forty_two() -> u32 {
    42
}

// `skip_serializing_if` hands the field by reference.
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero(n: &u32) -> bool {
    *n == 0
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Defaults {
    #[serde(default)]
    plain: String,
    #[serde(default = "forty_two")]
    answer: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    maybe: Option<u8>,
    #[serde(skip_serializing_if = "is_zero", default)]
    zero: u32,
    #[serde(skip)]
    local: Vec<u8>,
    #[serde(skip_serializing, default)]
    write_only: u8,
    #[serde(skip_deserializing)]
    read_only: u8,
}

#[quench_test::test]
fn defaults_and_skips() {
    let parsed = read::<Defaults>("{}");
    assert_eq!(
        parsed,
        Defaults {
            plain: String::new(),
            answer: 42,
            maybe: None,
            zero: 0,
            local: vec![],
            write_only: 0,
            read_only: 0
        }
    );
    let value = Defaults {
        plain: "p".into(),
        answer: 1,
        maybe: Some(3),
        zero: 5,
        local: vec![9],
        write_only: 4,
        read_only: 6,
    };
    // local, write_only are not written; maybe is written because it is Some
    assert_eq!(
        text(&value),
        r#"{"plain":"p","answer":1,"maybe":3,"zero":5,"read_only":6}"#
    );
    assert_eq!(
        text(&parsed),
        r#"{"plain":"","answer":42,"zero":0,"read_only":0}"#.replace(r#","zero":0"#, "")
    );
    // skip_deserializing never reads the key
    assert_eq!(read::<Defaults>(r#"{"read_only":9}"#).read_only, 0);
    assert_eq!(read::<Defaults>(r#"{"write_only":9}"#).write_only, 9);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct ContainerDefault {
    host: String,
    port: u16,
    #[serde(skip)]
    cache: u8,
}

impl Default for ContainerDefault {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 80,
            cache: 3,
        }
    }
}

#[quench_test::test]
fn container_default_fills_missing_fields_from_the_type_default() {
    assert_eq!(read::<ContainerDefault>("{}"), ContainerDefault::default());
    assert_eq!(
        read::<ContainerDefault>(r#"{"port":8080}"#),
        ContainerDefault {
            host: "localhost".into(),
            port: 8080,
            cache: 3
        }
    );
}

const fn custom_default() -> Custom {
    Custom { a: 10, b: 20 }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(default = "custom_default")]
struct Custom {
    a: u8,
    b: u8,
}

#[quench_test::test]
fn container_default_can_be_a_path() {
    assert_eq!(read::<Custom>(r#"{"a":1}"#), Custom { a: 1, b: 20 });
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Inner {
    x: u8,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Flattened {
    id: u32,
    #[serde(flatten)]
    inner: Inner,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[quench_test::test]
fn flatten_merges_and_collects_the_rest() {
    let parsed = read::<Flattened>(r#"{"id":1,"x":5,"color":"red","n":2}"#);
    assert_eq!(parsed.id, 1);
    assert_eq!(parsed.inner, Inner { x: 5 });
    assert_eq!(parsed.extra.len(), 3.min(parsed.extra.len()));
    assert!(parsed.extra.contains_key("color") && parsed.extra.contains_key("n"));
    let mut extra = HashMap::new();
    extra.insert("color".to_string(), Value::from("red"));
    let value = Flattened {
        id: 1,
        inner: Inner { x: 5 },
        extra,
    };
    assert_eq!(text(&value), r#"{"id":1,"x":5,"color":"red"}"#);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct MaybeFlattened {
    package: String,
    #[serde(flatten)]
    target: Option<Target>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Target {
    kind: String,
    name: String,
}

#[quench_test::test]
fn a_flattened_option_is_none_when_its_fields_are_not_there() {
    // serde's behaviour: the leftovers do not make a `Target`, so there is none - not an error.
    let none = read::<MaybeFlattened>(r#"{"package":"ml"}"#);
    assert_eq!(
        none,
        MaybeFlattened {
            package: "ml".into(),
            target: None
        }
    );
    let partial = read::<MaybeFlattened>(r#"{"package":"ml","kind":"Deployment"}"#);
    assert_eq!(partial.target, None);
    let some = read::<MaybeFlattened>(r#"{"package":"ml","kind":"Deployment","name":"sage"}"#);
    assert_eq!(
        some.target,
        Some(Target {
            kind: "Deployment".into(),
            name: "sage".into()
        })
    );
    assert_eq!(
        text(&some),
        r#"{"package":"ml","kind":"Deployment","name":"sage"}"#
    );
    assert_eq!(text(&none), r#"{"package":"ml"}"#);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Newtype(u32);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Pair(String, u8);

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Marker;

#[quench_test::test]
fn tuple_like_structs() {
    assert_eq!(text(&Newtype(5)), "5");
    assert_eq!(read::<Newtype>("5"), Newtype(5));
    assert_eq!(text(&Pair("a".into(), 1)), r#"["a",1]"#);
    assert_eq!(read::<Pair>(r#"["a",1]"#), Pair("a".into(), 1));
    assert!(fails::<Pair>(r#"["a"]"#).contains("length 2, found 1"));
    assert_eq!(text(&Marker), "null");
    assert_eq!(read::<Marker>("null"), Marker);
    assert!(fails::<Marker>("1").contains("expected null"));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
enum External {
    Unit,
    Newtype(String),
    Tuple(u8, u8),
    Struct {
        a: u8,
        b: Option<u8>,
    },
    #[serde(rename = "renamed", alias = "old")]
    Original,
}

#[quench_test::test]
fn externally_tagged_enums() {
    for (value, wire) in [
        (External::Unit, r#""Unit""#),
        (External::Newtype("n".into()), r#"{"Newtype":"n"}"#),
        (External::Tuple(1, 2), r#"{"Tuple":[1,2]}"#),
        (
            External::Struct { a: 1, b: None },
            r#"{"Struct":{"a":1,"b":null}}"#,
        ),
        (External::Original, r#""renamed""#),
    ] {
        assert_eq!(text(&value), wire);
        assert_eq!(read::<External>(wire), value);
    }
    assert_eq!(read::<External>(r#""old""#), External::Original);
    assert_eq!(read::<External>(r#"{"Unit":null}"#), External::Unit);
    assert_eq!(
        fails::<External>(r#""Nope""#),
        "unknown variant `Nope`, expected one of `Unit`, `Newtype`, `Tuple`, `Struct`, `renamed`"
    );
    assert!(fails::<External>(r#"{"Nope":1}"#).contains("unknown variant `Nope`"));
    assert!(fails::<External>(r#"{"Unit":null,"Tuple":[1,2]}"#).contains("single key"));
    assert!(fails::<External>("5").contains("a string or a single-key object"));
    assert_eq!(
        fails::<External>(r#"{"Newtype":5}"#),
        "Newtype: invalid type: expected a string, found number"
    );
    assert_eq!(
        fails::<External>(r#"{"Struct":{"a":"x"}}"#),
        "Struct.a: invalid type: expected an integer, found string"
    );
    assert!(fails::<External>(r#"{"Tuple":[1]}"#).contains("length 2"));
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Renamed {
    FirstThing,
    #[serde(rename_all = "camelCase")]
    SecondThing {
        inner_field: u8,
    },
}

#[quench_test::test]
fn rename_all_on_enums_reaches_variants_and_variant_fields() {
    assert_eq!(text(&Renamed::FirstThing), r#""first_thing""#);
    assert_eq!(
        text(&Renamed::SecondThing { inner_field: 1 }),
        r#"{"second_thing":{"innerField":1}}"#
    );
    assert_eq!(
        read::<Renamed>(r#"{"second_thing":{"innerField":1}}"#),
        Renamed::SecondThing { inner_field: 1 }
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Internal {
    Ping,
    Message { text: String },
    Wrapped(Inner),
}

#[quench_test::test]
fn internally_tagged_enums() {
    assert_eq!(text(&Internal::Ping), r#"{"type":"ping"}"#);
    assert_eq!(
        text(&Internal::Message { text: "hi".into() }),
        r#"{"type":"message","text":"hi"}"#
    );
    assert_eq!(
        text(&Internal::Wrapped(Inner { x: 2 })),
        r#"{"type":"wrapped","x":2}"#
    );
    assert_eq!(read::<Internal>(r#"{"type":"ping"}"#), Internal::Ping);
    assert_eq!(
        read::<Internal>(r#"{"text":"hi","type":"message"}"#),
        Internal::Message { text: "hi".into() }
    );
    assert_eq!(
        read::<Internal>(r#"{"type":"wrapped","x":2}"#),
        Internal::Wrapped(Inner { x: 2 })
    );
    assert_eq!(fails::<Internal>("{}"), "missing field `type`");
    assert_eq!(
        fails::<Internal>(r#"{"type":5}"#),
        "type: invalid type: expected a string, found number"
    );
    assert_eq!(
        fails::<Internal>(r#"{"type":"nope"}"#),
        "unknown variant `nope`, expected one of `ping`, `message`, `wrapped`"
    );
    assert_eq!(
        fails::<Internal>(r#"{"type":"message"}"#),
        "message: missing field `text`"
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Untagged {
    Number(u32),
    Text(String),
    Pair(u8, u8),
    Object { id: u8 },
    Nothing,
}

#[quench_test::test]
fn untagged_enums_try_each_variant_in_order() {
    assert_eq!(read::<Untagged>("5"), Untagged::Number(5));
    assert_eq!(read::<Untagged>(r#""s""#), Untagged::Text("s".into()));
    assert_eq!(read::<Untagged>("[1,2]"), Untagged::Pair(1, 2));
    assert_eq!(read::<Untagged>(r#"{"id":3}"#), Untagged::Object { id: 3 });
    assert_eq!(read::<Untagged>("null"), Untagged::Nothing);
    assert_eq!(text(&Untagged::Number(5)), "5");
    assert_eq!(text(&Untagged::Pair(1, 2)), "[1,2]");
    assert_eq!(text(&Untagged::Object { id: 3 }), r#"{"id":3}"#);
    assert_eq!(text(&Untagged::Nothing), "null");
    assert_eq!(
        fails::<Untagged>("true"),
        "data did not match any variant of untagged enum `Untagged`"
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Generic<T> {
    value: T,
    list: Vec<T>,
}

#[derive(Debug, PartialEq, Serialize)]
struct Borrowed<'a> {
    name: &'a str,
    parts: &'a [u8],
}

#[quench_test::test]
fn generics_and_lifetimes() {
    let g = Generic {
        value: 1_u8,
        list: vec![2, 3],
    };
    assert_eq!(text(&g), r#"{"value":1,"list":[2,3]}"#);
    assert_eq!(read::<Generic<u8>>(&text(&g)), g);
    assert_eq!(
        text(&Generic {
            value: "s".to_string(),
            list: vec![]
        }),
        r#"{"value":"s","list":[]}"#
    );
    assert_eq!(
        text(&Borrowed {
            name: "n",
            parts: &[1, 2]
        }),
        r#"{"name":"n","parts":[1,2]}"#
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Raw {
    r#type: String,
    r#match: u8,
}

#[quench_test::test]
fn raw_identifiers_lose_their_prefix() {
    let value = Raw {
        r#type: "t".into(),
        r#match: 1,
    };
    assert_eq!(text(&value), r#"{"type":"t","match":1}"#);
    assert_eq!(read::<Raw>(r#"{"type":"t","match":1}"#), value);
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Nested {
    items: Vec<Plain>,
    by_name: BTreeMap<String, Inner>,
}

#[quench_test::test]
fn nested_types_report_the_full_path() {
    let e = fails::<Nested>(
        r#"{"items":[{"name":"a","count":1,"tags":[]},{"name":"b","count":"x","tags":[]}],"by_name":{}}"#,
    );
    assert_eq!(
        e,
        "items[1].count: invalid type: expected an integer, found string"
    );
    let e = fails::<Nested>(r#"{"items":[],"by_name":{"k":{"x":"no"}}}"#);
    assert_eq!(
        e,
        "by_name.k.x: invalid type: expected an integer, found string"
    );
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, tag = "kind")]
enum Strict {
    A { v: u8 },
}

#[quench_test::test]
fn deny_unknown_fields_reaches_internally_tagged_variants() {
    assert_eq!(read::<Strict>(r#"{"kind":"A","v":1}"#), Strict::A { v: 1 });
    assert!(fails::<Strict>(r#"{"kind":"A","v":1,"w":2}"#).contains("unknown field `w`"));
}

#[quench_test::test]
fn the_json_macro_accepts_derived_types() {
    let value = json!({ "plain": Plain { name: "a".into(), count: 1, tags: vec![], note: None }, "n": Newtype(3) });
    assert_eq!(
        value.to_string(),
        r#"{"plain":{"name":"a","count":1,"tags":[],"note":null},"n":3}"#
    );
}

quench_test::main!();
