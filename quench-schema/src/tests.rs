use super::*;

fn schema(text: &str) -> Schema {
    Schema::new(&json::parse(text).expect("json")).unwrap_or_else(|e| panic!("{text}: {e}"))
}

fn problems(schema_text: &str, instance: &str) -> Vec<(String, String)> {
    schema(schema_text)
        .validate(&json::parse(instance).expect("json"))
        .iter()
        .map(|e| (e.instance_path().to_string(), e.message().to_string()))
        .collect()
}

fn messages(schema_text: &str, instance: &str) -> Vec<String> {
    problems(schema_text, instance)
        .into_iter()
        .map(|(_, m)| m)
        .collect()
}

fn valid(schema_text: &str, instance: &str) -> bool {
    problems(schema_text, instance).is_empty()
}

#[test]
fn types() {
    assert_eq!(
        messages(r#"{"type":"string"}"#, "5"),
        [r#"5 is not of type "string""#]
    );
    assert_eq!(
        messages(r#"{"type":["string","null"]}"#, "5"),
        [r#"5 is not of types "null", "string""#]
    );
    assert_eq!(
        messages(r#"{"type":["integer","string","null"]}"#, "true"),
        [r#"true is not of types "null", "integer", "string""#]
    );
    assert!(valid(r#"{"type":"integer"}"#, "2.0"));
    assert!(valid(r#"{"type":"integer"}"#, "-3"));
    assert!(!valid(r#"{"type":"integer"}"#, "1.5"));
    assert!(valid(r#"{"type":"number"}"#, "1.5") && valid(r#"{"type":"number"}"#, "7"));
    for (kind, ok, bad) in [
        ("null", "null", "0"),
        ("boolean", "false", "1"),
        ("array", "[]", "{}"),
        ("object", "{}", "[]"),
        ("string", r#""x""#, "1"),
    ] {
        let s = format!(r#"{{"type":"{kind}"}}"#);
        assert!(valid(&s, ok) && !valid(&s, bad), "{kind}");
    }
    assert!(
        !valid(r#"{"type":"nonsense"}"#, "1"),
        "an unknown type matches nothing"
    );
    assert!(matches!(
        schema(r#"{"type":"string"}"#).validate(&Value::Null)[0].kind(),
        ErrorKind::Type { expected } if expected == &["string".to_string()]
    ));
}

#[test]
fn enums_and_constants() {
    assert_eq!(
        messages(r#"{"enum":["a"]}"#, r#""c""#),
        [r#""c" is not one of "a""#]
    );
    assert_eq!(
        messages(r#"{"enum":["a","b"]}"#, r#""c""#),
        [r#""c" is not one of "a" or "b""#]
    );
    assert_eq!(
        messages(r#"{"enum":["a","b","c"]}"#, r#""z""#),
        [r#""z" is not one of "a", "b" or "c""#]
    );
    assert_eq!(
        messages(r#"{"enum":[1,null,true,{"a":1}]}"#, r#""z""#),
        [r#""z" is not one of 1, null or 2 other candidates"#]
    );
    assert!(valid(r#"{"enum":[1,2]}"#, "2.0"), "1.0 is the number 1");
    assert!(valid(r#"{"enum":[{"a":[1,2]}]}"#, r#"{"a":[1,2.0]}"#));
    assert_eq!(messages(r#"{"const":3}"#, "4"), ["3 was expected"]);
    assert!(valid(r#"{"const":[1,{"a":null}]}"#, r#"[1,{"a":null}]"#));
    assert!(!valid(r#"{"const":{"a":1}}"#, r#"{"a":1,"b":2}"#));
    assert!(!valid(r#"{"const":"1"}"#, "1"));
}

#[test]
fn strings() {
    assert_eq!(
        messages(r#"{"minLength":3}"#, r#""ab""#),
        [r#""ab" is shorter than 3 characters"#]
    );
    assert_eq!(
        messages(r#"{"minLength":1}"#, r#""""#),
        [r#""" is shorter than 1 character"#]
    );
    assert_eq!(
        messages(r#"{"maxLength":2}"#, r#""abc""#),
        [r#""abc" is longer than 2 characters"#]
    );
    assert_eq!(
        messages(r#"{"maxLength":1}"#, r#""ab""#),
        [r#""ab" is longer than 1 character"#]
    );
    assert!(
        valid(r#"{"maxLength":2}"#, r#""żó""#),
        "lengths count characters"
    );
    assert_eq!(
        messages(r#"{"pattern":"^[a-z]+$"}"#, r#""ABC""#),
        [r#""ABC" does not match "^[a-z]+$""#]
    );
    assert!(
        valid(r#"{"pattern":"a"}"#, r#""xax""#),
        "a pattern is unanchored"
    );
    assert!(
        valid(r#"{"pattern":"a"}"#, "5"),
        "string keywords ignore other types"
    );
    assert!(valid(r#"{"minLength":5}"#, "1"));
    assert!(
        valid(r#"{"format":"date-time"}"#, r#""nope""#),
        "format is an annotation"
    );
}

#[test]
fn numbers() {
    assert_eq!(
        messages(r#"{"minimum":5}"#, "3"),
        ["3 is less than the minimum of 5"]
    );
    assert_eq!(
        messages(r#"{"maximum":5}"#, "7"),
        ["7 is greater than the maximum of 5"]
    );
    assert_eq!(
        messages(r#"{"exclusiveMinimum":5}"#, "5"),
        ["5 is less than or equal to the minimum of 5"]
    );
    assert_eq!(
        messages(r#"{"exclusiveMaximum":5}"#, "5"),
        ["5 is greater than or equal to the maximum of 5"]
    );
    assert_eq!(
        messages(r#"{"minimum":1.5}"#, "1"),
        ["1 is less than the minimum of 1.5"]
    );
    assert!(valid(r#"{"minimum":5,"maximum":5}"#, "5"));
    assert_eq!(
        messages(r#"{"multipleOf":3}"#, "4"),
        ["4 is not a multiple of 3"]
    );
    assert!(valid(r#"{"multipleOf":3}"#, "9") && valid(r#"{"multipleOf":0.5}"#, "2.5"));
    assert_eq!(
        messages(r#"{"multipleOf":0.5}"#, "0.7"),
        ["0.7 is not a multiple of 0.5"]
    );
    assert!(
        valid(r#"{"multipleOf":0.1}"#, "0.3"),
        "floating point noise is tolerated"
    );
    assert!(
        valid(r#"{"minimum":5}"#, r#""x""#),
        "number keywords ignore other types"
    );
    assert!(matches!(
        schema(r#"{"minimum":5}"#).validate(&json::parse("1").unwrap())[0].kind(),
        ErrorKind::Minimum { limit } if (*limit - 5.0).abs() < f64::EPSILON
    ));
}

#[test]
fn arrays() {
    assert_eq!(
        messages(r#"{"minItems":2}"#, "[1]"),
        ["[1] has less than 2 items"]
    );
    assert_eq!(
        messages(r#"{"minItems":1}"#, "[]"),
        ["[] has less than 1 item"]
    );
    assert_eq!(
        messages(r#"{"maxItems":1}"#, "[1,2]"),
        ["[1,2] has more than 1 item"]
    );
    assert_eq!(
        messages(r#"{"maxItems":0}"#, "[1]"),
        ["[1] has more than 0 items"]
    );
    assert_eq!(
        messages(r#"{"uniqueItems":true}"#, "[1,1]"),
        ["[1,1] has non-unique elements"]
    );
    assert!(valid(
        r#"{"uniqueItems":true}"#,
        "[1,2,{\"a\":1},{\"a\":2}]"
    ));
    assert!(!valid(r#"{"uniqueItems":true}"#, "[1,1.0]"));
    assert!(valid(r#"{"uniqueItems":false}"#, "[1,1]"));
    assert_eq!(
        problems(r#"{"items":{"type":"string"}}"#, r#"["a",1,2]"#),
        [
            ("/1".into(), r#"1 is not of type "string""#.into()),
            ("/2".into(), r#"2 is not of type "string""#.into())
        ]
    );
    assert_eq!(
        problems(
            r#"{"prefixItems":[{"type":"string"},{"type":"integer"}],"items":{"type":"boolean"}}"#,
            r#"[1,"a",5]"#
        ),
        [
            ("/0".into(), r#"1 is not of type "string""#.into()),
            ("/1".into(), r#""a" is not of type "integer""#.into()),
            ("/2".into(), r#"5 is not of type "boolean""#.into())
        ]
    );
    assert_eq!(
        messages(r#"{"contains":{"type":"string"}}"#, "[1]"),
        ["None of [1] are valid under the given schema"]
    );
    assert!(valid(r#"{"contains":{"type":"string"}}"#, r#"[1,"a"]"#));
}

#[test]
fn objects() {
    let s = r#"{"properties":{"a":{"type":"string"},"b":{"type":"integer"}},"required":["a","c"],"additionalProperties":false}"#;
    assert_eq!(
        problems(s, r#"{"a":1,"x":1,"y":2}"#),
        [
            (String::new(), r#""c" is a required property"#.into()),
            ("/a".into(), r#"1 is not of type "string""#.into()),
            (
                String::new(),
                "Additional properties are not allowed ('x', 'y' were unexpected)".into()
            ),
        ]
    );
    assert_eq!(
        messages(r#"{"additionalProperties":false}"#, r#"{"x":1}"#),
        ["Additional properties are not allowed ('x' was unexpected)"]
    );
    assert!(matches!(
        schema(s).validate(&json::parse(r#"{"a":"s","x":1,"c":1}"#).unwrap())[0].kind(),
        ErrorKind::AdditionalProperties { unexpected } if unexpected == &["x".to_string(), "c".to_string()]
    ));
    assert_eq!(
        problems(
            r#"{"additionalProperties":{"type":"integer"},"properties":{"a":{}}}"#,
            r#"{"a":"s","x":"no"}"#
        ),
        [("/x".into(), r#""no" is not of type "integer""#.into())]
    );
    assert_eq!(
        problems(
            r#"{"patternProperties":{"^x":{"type":"integer"}},"additionalProperties":false}"#,
            r#"{"xa":"s","y":1}"#
        ),
        [
            ("/xa".into(), r#""s" is not of type "integer""#.into()),
            (
                String::new(),
                "Additional properties are not allowed ('y' was unexpected)".into()
            )
        ]
    );
    assert_eq!(
        messages(r#"{"minProperties":2}"#, r#"{"a":1}"#),
        [r#"{"a":1} has less than 2 properties"#]
    );
    assert_eq!(
        messages(r#"{"minProperties":1}"#, "{}"),
        ["{} has less than 1 property"]
    );
    assert_eq!(
        messages(r#"{"maxProperties":1}"#, r#"{"a":1,"b":2}"#),
        [r#"{"a":1,"b":2} has more than 1 property"#]
    );
    assert_eq!(
        messages(r#"{"maxProperties":0}"#, r#"{"a":1}"#),
        [r#"{"a":1} has more than 0 properties"#]
    );
    assert_eq!(
        messages(
            r#"{"dependentRequired":{"a":["b","c"]}}"#,
            r#"{"a":1,"c":1}"#
        ),
        [r#""b" is a required property"#]
    );
    assert!(valid(r#"{"dependentRequired":{"a":["b"]}}"#, r#"{"z":1}"#));
    assert_eq!(
        messages(r#"{"propertyNames":{"minLength":2}}"#, r#"{"a":1,"bb":2}"#),
        [r#""a" is shorter than 2 characters"#]
    );
    assert!(
        valid(r#"{"properties":{"a":{"type":"string"}}}"#, r#"{"b":1}"#),
        "absent properties are fine"
    );
}

#[test]
fn instance_paths_are_json_pointers() {
    assert_eq!(
        problems(
            r#"{"properties":{"a b":{"type":"string"},"c/d":{"type":"string"},"e~f":{"type":"string"},"g":{"items":{"type":"integer"}}}}"#,
            r#"{"a b":1,"c/d":2,"e~f":3,"g":[0,"x"]}"#
        )
        .iter()
        .map(|(p, _)| p.as_str())
        .collect::<Vec<_>>(),
        ["/a b", "/c~1d", "/e~0f", "/g/1"]
    );
}

#[test]
fn combinators() {
    assert_eq!(
        messages(
            r#"{"allOf":[{"type":"string"},{"minLength":5}]}"#,
            r#""ab""#
        ),
        [r#""ab" is shorter than 5 characters"#]
    );
    assert_eq!(
        messages(
            r#"{"anyOf":[{"type":"string"},{"type":"integer"}]}"#,
            "true"
        ),
        ["true is not valid under any of the schemas listed in the 'anyOf' keyword"]
    );
    assert!(valid(
        r#"{"anyOf":[{"type":"string"},{"type":"integer"}]}"#,
        "1"
    ));
    assert_eq!(
        messages(
            r#"{"oneOf":[{"type":"string"},{"type":"integer"}]}"#,
            "true"
        ),
        ["true is not valid under any of the schemas listed in the 'oneOf' keyword"]
    );
    assert_eq!(
        messages(r#"{"oneOf":[{"type":"integer"},{"type":"number"}]}"#, "1"),
        ["1 is valid under more than one of the schemas listed in the 'oneOf' keyword"]
    );
    assert!(valid(
        r#"{"oneOf":[{"type":"integer"},{"type":"string"}]}"#,
        "1"
    ));
    assert_eq!(
        messages(r#"{"not":{"type":"string"}}"#, r#""a""#),
        [r#"{"type":"string"} is not allowed for "a""#]
    );
    let conditional = r#"{"if":{"properties":{"a":{"const":1}},"required":["a"]},"then":{"required":["b"]},"else":{"required":["c"]}}"#;
    assert_eq!(
        messages(conditional, r#"{"a":1}"#),
        [r#""b" is a required property"#]
    );
    assert_eq!(
        messages(conditional, r#"{"a":2}"#),
        [r#""c" is a required property"#]
    );
    assert!(valid(conditional, r#"{"a":1,"b":1}"#));
}

#[test]
fn boolean_schemas_and_references() {
    assert!(valid("true", "1") && !valid("false", "1"));
    assert_eq!(messages("false", "1"), ["False schema does not allow 1"]);
    assert_eq!(
        problems(r#"{"properties":{"a":false}}"#, r#"{"a":1}"#)[0].0,
        "/a"
    );
    assert!(matches!(
        schema("false").validate(&Value::Null)[0].kind(),
        ErrorKind::False
    ));

    let with_ref = r##"{"$ref":"#/definitions/x","definitions":{"x":{"type":"string"}}}"##;
    assert_eq!(messages(with_ref, "1"), [r#"1 is not of type "string""#]);
    assert!(valid(with_ref, r#""ok""#));
    let defs = r##"{"properties":{"a":{"$ref":"#/$defs/n"}},"$defs":{"n":{"type":"integer"}}}"##;
    assert_eq!(problems(defs, r#"{"a":"x"}"#)[0].0, "/a");
    // A reference to nowhere constrains nothing; a cycle gives up instead of looping.
    assert!(valid(r##"{"$ref":"#/missing"}"##, "1"));
    assert!(valid(r#"{"$ref":"https://example.com/other"}"#, "1"));
    assert!(valid(r##"{"$ref":"#"}"##, "1"));
    assert!(valid(
        r##"{"properties":{"n":{"$ref":"#"}}}"##,
        r#"{"n":{"n":{"n":1}}}"#
    ));
}

#[test]
fn unknown_keywords_are_annotations() {
    assert!(valid(
        r#"{"description":"x","x-kubernetes-int-or-string":true,"nullable":true,"default":3,"title":"t"}"#,
        r#""anything""#
    ));
}

#[test]
fn bad_schemas_are_refused() {
    for (text, wanted) in [
        ("5", "an object or a boolean"),
        (r#"{"pattern":"("}"#, "not a valid pattern"),
        (r#"{"pattern":5}"#, "`pattern` is a string"),
        (r#"{"patternProperties":{"(":{}}}"#, "not a valid pattern"),
        (r#"{"type":5}"#, "`type`"),
        (r#"{"type":[1]}"#, "`type`"),
        (r#"{"required":"a"}"#, "`required`"),
        (r#"{"required":[1]}"#, "`required`"),
        (r#"{"enum":"a"}"#, "`enum`"),
        (r#"{"properties":[]}"#, "`properties` is an object"),
        (r#"{"properties":{"a":5}}"#, "an object or a boolean"),
        (r#"{"allOf":{}}"#, "`allOf` is an array"),
        (r#"{"anyOf":[5]}"#, "an object or a boolean"),
        (r#"{"items":5}"#, "an object or a boolean"),
        (r#"{"minLength":-1}"#, "non-negative integer"),
        (r#"{"minItems":"x"}"#, "non-negative integer"),
        (r#"{"minimum":"x"}"#, "`minimum` is a number"),
        (r#"{"multipleOf":"x"}"#, "`multipleOf` is a number"),
    ] {
        let error = Schema::new(&json::parse(text).unwrap()).unwrap_err();
        assert!(error.to_string().contains(wanted), "{text}: {error}");
        assert!(error.to_string().starts_with("invalid schema:"));
    }
    let mut deep = Value::Bool(true);
    for _ in 0..200 {
        let mut map = Map::new();
        map.insert("not", deep);
        deep = Value::Object(map);
    }
    assert!(Schema::new(&deep).is_err());
}

#[test]
fn deep_nesting_in_the_instance_is_fine_and_cycles_do_not_hang() {
    let schema = Schema::new(&json::parse(r##"{"$ref":"#"}"##).unwrap()).unwrap();
    assert!(schema.is_valid(&Value::Null));
    let self_all = Schema::new(&json::parse(r##"{"allOf":[{"$ref":"#"}]}"##).unwrap()).unwrap();
    assert!(
        self_all.is_valid(&Value::Bool(true)),
        "a cycle stops at the depth limit"
    );
}

#[test]
fn errors_display_and_compare() {
    let error = schema(r#"{"type":"string"}"#)
        .validate(&Value::Null)
        .remove(0);
    assert_eq!(error.to_string(), r#"null is not of type "string""#);
    assert_eq!(error.instance_path(), "");
    assert_eq!(error.clone(), error);
    let _: &dyn std::error::Error = &error;
    assert!(format!("{:?}", schema("true")).contains("Schema"));
    assert!(schema(r#"{"type":"string"}"#).is_valid(&Value::String("x".into())));
}
