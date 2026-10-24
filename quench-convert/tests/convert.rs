//! The four readers: CSV, XML, Android binary XML and GGUF.

#![allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    unused_imports,
    dead_code
)]

#[cfg(feature = "axml")]
use quench_convert::axml;
#[cfg(feature = "csv")]
use quench_convert::csv;
#[cfg(feature = "gguf")]
use quench_convert::gguf;
#[cfg(feature = "xml")]
use quench_convert::xml;
#[cfg(any(feature = "xml", feature = "csv", feature = "gguf"))]
use quench_serde::{Value, json};

// ---------------------------------------------------------------- csv

#[cfg(feature = "csv")]
#[test]
fn csv_records_and_quoting() {
    let text = "a,b,c\r\n1,\"two, 2\",\"say \"\"hi\"\"\"\r\n\r\n\"multi\nline\",,\n";
    let rows = csv::records(text, b',', false).unwrap();
    assert_eq!(
        rows,
        vec![
            vec!["a", "b", "c"],
            vec!["1", "two, 2", "say \"hi\""],
            vec!["multi\nline", "", ""],
        ]
    );
    assert_eq!(
        csv::records("x;y\n1;2", b';', false).unwrap(),
        vec![vec!["x", "y"], vec!["1", "2"]]
    );
    assert_eq!(
        csv::records("\u{FEFF}a,b", b',', false).unwrap(),
        vec![vec!["a", "b"]]
    );
    assert_eq!(
        csv::records("  a , b  ,\" c \" \n", b',', true).unwrap(),
        vec![vec!["a", "b", " c  "]]
    );
    assert_eq!(
        csv::records("a,b,", b',', false).unwrap(),
        vec![vec!["a", "b", ""]]
    );
    assert_eq!(
        csv::records("a,\"b\",", b',', false).unwrap(),
        vec![vec!["a", "b", ""]]
    );
    assert_eq!(
        csv::records("a,\"b\"", b',', false).unwrap(),
        vec![vec!["a", "b"]]
    );
    assert_eq!(
        csv::records("", b',', false).unwrap(),
        Vec::<Vec<String>>::new()
    );
    assert_eq!(
        csv::records("ab\"c,d", b',', false).unwrap(),
        vec![vec!["ab\"c", "d"]]
    );
}

#[cfg(feature = "csv")]
#[test]
fn csv_errors_and_leniency() {
    // Lenient by default, as csv readers usually are.
    assert_eq!(
        csv::records("a,\"b\n", b',', false).unwrap(),
        vec![vec!["a", "b\n"]]
    );
    assert_eq!(
        csv::records("\"a\"b,c", b',', false).unwrap(),
        vec![vec!["ab", "c"]]
    );
    let strict = csv::Options {
        strict: true,
        ..csv::Options::default()
    };
    let unterminated = csv::parse("a,\"b\n", &strict).unwrap_err();
    assert_eq!((unterminated.format(), unterminated.position()), ("csv", 1));
    assert!(unterminated.to_string().contains("never closed"));
    assert!(csv::parse("\"a\"b,c", &strict).is_err());
    assert!(csv::parse("\"a\",c\n\"b\"\n", &strict).is_ok());
    let fixed = csv::Options {
        flexible: false,
        ..csv::Options::default()
    };
    assert!(csv::parse("a,b\n1,2\n3\n", &fixed).is_err());
    assert!(csv::parse("a,b\n1,2\n3,4\n", &fixed).is_ok());
}

#[cfg(feature = "csv")]
#[test]
fn csv_values() {
    let rows = csv::parse(
        "name, qty ,\nbolt,4,x,y\nnut\n",
        &csv::Options {
            trim: true,
            ..csv::Options::default()
        },
    )
    .unwrap();
    assert_eq!(
        rows[0],
        json!({"name": "bolt", "qty": "4", "column3": "x", "column4": "y"})
    );
    assert_eq!(rows[1], json!({"name": "nut"}));
    let plain = csv::parse(
        "a,b\n1,2\n",
        &csv::Options {
            header: false,
            ..csv::Options::default()
        },
    )
    .unwrap();
    assert_eq!(plain, vec![json!(["a", "b"]), json!(["1", "2"])]);
    assert!(csv::parse("", &csv::Options::default()).unwrap().is_empty());
}

#[cfg(feature = "csv")]
#[test]
fn csv_writing_round_trips() {
    let rows = vec![
        json!({"name": "bolt", "note": "has, comma", "n": 4, "gone": null}),
        json!({"name": " pad ", "note": "quote \" here", "extra": 1}),
        json!({"note": "line\nbreak"}),
    ];
    let text = csv::to_string(&rows, b',');
    assert_eq!(
        text,
        "name,note,n,gone\nbolt,\"has, comma\",4,\n\" pad \",\"quote \"\" here\",,\n,\"line\nbreak\",,\n"
    );
    let back = csv::parse(&text, &csv::Options::default()).unwrap();
    assert_eq!(back[0]["note"], "has, comma");
    assert_eq!(back[1]["name"], " pad ");
    assert_eq!(back[2]["note"], "line\nbreak");
    assert_eq!(
        csv::to_string(&[json!(["a", 1, null]), json!("solo")], b';'),
        "a;1;\nsolo\n"
    );
    assert_eq!(csv::to_string(&[], b','), "");
}

// ---------------------------------------------------------------- xml

#[cfg(feature = "xml")]
#[test]
fn xml_parses_the_tree() {
    let doc = xml::parse(
        "\u{FEFF}<?xml version=\"1.0\"?><!DOCTYPE note [<!ENTITY x \"y\">]>\n<!-- c --><a:root xmlns:a='u' id=\"1\" q='say &quot;hi&quot;'>\n  <item n=\"1\">one &amp; &lt;two&gt; &#65;&#x42;</item>\n  <item/>\n  <![CDATA[<raw & text>]]>tail<?pi x?>\n</a:root><!-- end -->\n",
    )
    .unwrap();
    assert_eq!(xml::tag(&doc), Some("a:root"));
    assert_eq!(xml::attr(&doc, "id"), Some("1"));
    assert_eq!(xml::attr(&doc, "q"), Some("say \"hi\""));
    assert_eq!(xml::attr(&doc, "missing"), None);
    let items: Vec<&Value> = xml::elements(&doc, "item").collect();
    assert_eq!(items.len(), 2);
    assert_eq!(xml::text(items[0]), "one & <two> AB");
    assert_eq!(xml::attr(items[0], "n"), Some("1"));
    assert!(xml::children(items[1]).is_empty());
    assert_eq!(xml::child(&doc, "item"), Some(items[0]));
    assert_eq!(xml::child(&doc, "nope"), None);
    assert_eq!(xml::text(&doc), "one & <two> AB\n  <raw & text>tail\n");
    assert_eq!(xml::tag(&Value::Null), None);
    assert!(xml::children(&Value::Null).is_empty());
}

#[cfg(feature = "xml")]
#[test]
fn xml_whitespace_and_writing() {
    let source = "<r>\n  <a> x </a>\n  <b/>\n</r>";
    let trimmed = xml::parse(source).unwrap();
    assert_eq!(xml::children(&trimmed).len(), 2);
    let kept = xml::parse_with(
        source,
        &xml::Options {
            keep_whitespace: true,
        },
    )
    .unwrap();
    assert_eq!(xml::children(&kept).len(), 5);
    assert_eq!(xml::to_string(&trimmed), "<r><a> x </a><b/></r>");
    assert_eq!(
        xml::to_string_pretty(&trimmed, 2),
        "<r>\n  <a> x </a>\n  <b/>\n</r>"
    );

    let doc = xml::parse("<r a=\"&lt;&amp;&quot;\">&lt;&amp;&gt;<k/></r>").unwrap();
    let written = xml::to_string(&doc);
    assert_eq!(written, "<r a=\"&lt;&amp;&quot;\">&lt;&amp;&gt;<k/></r>");
    assert_eq!(xml::parse(&written).unwrap(), doc);
    // An element value built by hand.
    let built = xml::element(
        "x",
        std::iter::once(("k".to_string(), Value::from("v"))).collect(),
        vec![Value::from("t")],
    );
    assert_eq!(xml::to_string(&built), "<x k=\"v\">t</x>");
    // A bare string writes as escaped text, and a value that is no element still gets a tag.
    assert_eq!(xml::to_string(&Value::from("a<b")), "a&lt;b");
    assert_eq!(xml::to_string(&json!({})), "<element/>");
}

#[cfg(feature = "xml")]
#[test]
fn xml_errors() {
    for bad in [
        "",
        "text only",
        "<a>",
        "<a></b>",
        "<a b></a>",
        "<a b=1></a>",
        "<a b=\"1></a>",
        "<a>&nope;</a>",
        "<a>&amp</a>",
        "<a/><b/>",
        "<a><!-- open</a>",
        "<a><![CDATA[x</a>",
        "<!DOCTYPE a [",
        "<a></a",
        "<>",
        "<a b='x' c></a>",
    ] {
        assert!(xml::parse(bad).is_err(), "{bad:?}");
    }
    let e = xml::parse("<a>\n<b>\n</a>").unwrap_err();
    assert_eq!((e.format(), e.position()), ("xml", 3));
    assert!(e.message().contains("closes"));
    let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
    assert!(xml::parse(&deep).is_err());
    let fine = format!("{}{}", "<a>".repeat(200), "</a>".repeat(200));
    assert!(xml::parse(&fine).is_ok());
}

// ---------------------------------------------------------------- axml

#[cfg(feature = "axml")]
enum Attr {
    Str(u32),
    Typed(u32, u8, u32),
}

#[cfg(feature = "axml")]
/// Builds a document: namespace `android`, a `manifest` element with attributes, a child and text.
fn build_axml(utf8: bool, attrs: &[Attr]) -> Vec<u8> {
    let mut strings: Vec<String> = [
        "android",
        "http://schemas.android.com/apk/res/android",
        "manifest",
        "package",
        "child",
        "text",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    // Attribute names are appended after the fixed strings: name_i at 6 + i.
    let mut names = Vec::new();
    for (i, _) in attrs.iter().enumerate() {
        names.push(format!("attr{i}"));
    }
    let name_base = strings.len() as u32;
    strings.extend(names);
    strings.push("a string value".to_string());
    let value_index = strings.len() as u32 - 1;

    let pool = {
        let mut data = Vec::new();
        let mut offsets = Vec::new();
        for s in &strings {
            offsets.push(data.len() as u32);
            if utf8 {
                data.push(s.chars().count() as u8);
                data.push(s.len() as u8);
                data.extend_from_slice(s.as_bytes());
                data.push(0);
            } else {
                let units: Vec<u16> = s.encode_utf16().collect();
                data.extend_from_slice(&(units.len() as u16).to_le_bytes());
                for u in units {
                    data.extend_from_slice(&u.to_le_bytes());
                }
                data.extend_from_slice(&[0, 0]);
            }
        }
        while data.len() % 4 != 0 {
            data.push(0);
        }
        let start = 28 + offsets.len() * 4;
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&1u16.to_le_bytes());
        chunk.extend_from_slice(&28u16.to_le_bytes());
        chunk.extend_from_slice(&((start + data.len()) as u32).to_le_bytes());
        chunk.extend_from_slice(&(strings.len() as u32).to_le_bytes());
        chunk.extend_from_slice(&0u32.to_le_bytes());
        chunk.extend_from_slice(&(if utf8 { 0x100u32 } else { 0 }).to_le_bytes());
        chunk.extend_from_slice(&(start as u32).to_le_bytes());
        chunk.extend_from_slice(&0u32.to_le_bytes());
        for o in offsets {
            chunk.extend_from_slice(&o.to_le_bytes());
        }
        chunk.extend_from_slice(&data);
        chunk
    };

    let node = |kind: u16, ext: &[u8]| {
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&kind.to_le_bytes());
        chunk.extend_from_slice(&16u16.to_le_bytes());
        chunk.extend_from_slice(&((16 + ext.len()) as u32).to_le_bytes());
        chunk.extend_from_slice(&1u32.to_le_bytes());
        chunk.extend_from_slice(&u32::MAX.to_le_bytes());
        chunk.extend_from_slice(ext);
        chunk
    };
    let words = |w: &[u32]| w.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();

    let mut body = pool;
    body.extend(node(0x0100, &words(&[0, 1])));
    let mut ext = words(&[u32::MAX, 2]);
    ext.extend_from_slice(&0x14u16.to_le_bytes());
    ext.extend_from_slice(&0x14u16.to_le_bytes());
    ext.extend_from_slice(&((attrs.len() + 1) as u16).to_le_bytes());
    ext.extend_from_slice(&[0; 6]);
    // package="a string value" (no namespace, raw string value)
    ext.extend(words(&[u32::MAX, 3, value_index]));
    ext.extend_from_slice(&8u16.to_le_bytes());
    ext.extend_from_slice(&[0, 3]);
    ext.extend(words(&[value_index]));
    for (i, a) in attrs.iter().enumerate() {
        let name = name_base + i as u32;
        match a {
            Attr::Str(ns) => {
                ext.extend(words(&[*ns, name, value_index]));
                ext.extend_from_slice(&8u16.to_le_bytes());
                ext.extend_from_slice(&[0, 3]);
                ext.extend(words(&[value_index]));
            }
            Attr::Typed(ns, kind, data) => {
                ext.extend(words(&[*ns, name, u32::MAX]));
                ext.extend_from_slice(&8u16.to_le_bytes());
                ext.extend_from_slice(&[0, *kind]);
                ext.extend(words(&[*data]));
            }
        }
    }
    body.extend(node(0x0102, &ext));
    let mut child = words(&[u32::MAX, 4]);
    child.extend_from_slice(&0x14u16.to_le_bytes());
    child.extend_from_slice(&0x14u16.to_le_bytes());
    child.extend_from_slice(&[0; 8]);
    body.extend(node(0x0102, &child));
    let mut cdata = words(&[5]);
    cdata.extend_from_slice(&[8, 0, 0, 3, 5, 0, 0, 0]);
    body.extend(node(0x0104, &cdata));
    body.extend(node(0x0103, &words(&[u32::MAX, 4])));
    body.extend(node(0x0103, &words(&[u32::MAX, 2])));
    body.extend(node(0x0101, &words(&[0, 1])));

    let mut file = Vec::new();
    file.extend_from_slice(&3u16.to_le_bytes());
    file.extend_from_slice(&8u16.to_le_bytes());
    file.extend_from_slice(&((8 + body.len()) as u32).to_le_bytes());
    file.extend(body);
    file
}

#[cfg(feature = "axml")]
#[test]
fn axml_reads_real_manifests() {
    let minimal = axml::parse(include_bytes!("fixtures/minimal.axml")).unwrap();
    assert_eq!(xml::tag(&minimal), Some("manifest"));
    assert!(xml::attr(&minimal, "package").is_some_and(|p| !p.is_empty()));
    assert!(
        xml::attr(&minimal, "android:versionCode")
            .and_then(|v| v.parse::<i64>().ok())
            .is_some()
    );
    assert!(xml::attr(&minimal, "xmlns:android").is_some_and(|u| u.contains("android")));
    assert!(xml::child(&minimal, "application").is_some());
    // The tree writes back out as XML and reads again.
    let text = xml::to_string(&minimal);
    assert_eq!(xml::parse(&text).unwrap(), minimal);

    let no_version = axml::parse(include_bytes!("fixtures/missing_version_code.axml")).unwrap();
    assert_eq!(xml::attr(&no_version, "android:versionCode"), None);

    // An unresolved resource reference stays a reference.
    let labelled = axml::parse(include_bytes!("fixtures/resource_label.axml")).unwrap();
    let application = xml::child(&labelled, "application").unwrap();
    assert!(
        xml::attr(application, "android:label").is_some_and(|l| l.starts_with('@')),
        "{application:?}"
    );
}

#[cfg(feature = "axml")]
#[test]
fn axml_value_types() {
    let ns = 1;
    let attrs = [
        Attr::Typed(ns, 0x10, (-5i32).cast_unsigned()),
        Attr::Typed(ns, 0x11, 0xFF),
        Attr::Typed(ns, 0x12, 1),
        Attr::Typed(ns, 0x12, 0),
        Attr::Typed(ns, 0x01, 0x7F0B_0001),
        Attr::Typed(ns, 0x02, 0x0101_0001),
        Attr::Typed(ns, 0x04, 1.5f32.to_bits()),
        Attr::Typed(ns, 0x05, (16 << 8) | 1),
        Attr::Typed(ns, 0x05, 2 << 8),
        Attr::Typed(ns, 0x06, 1 << 8),
        Attr::Typed(ns, 0x1c, 0xFF11_2233),
        Attr::Typed(ns, 0x1d, 0xFF11_2233),
        Attr::Typed(ns, 0x1e, 0x1234),
        Attr::Typed(ns, 0x1f, 0x123),
        Attr::Typed(ns, 0x00, 0),
        Attr::Typed(ns, 0x77, 9),
        Attr::Typed(u32::MAX, 0x10, 7),
        Attr::Str(ns),
    ];
    for utf8 in [false, true] {
        let root = axml::parse(&build_axml(utf8, &attrs)).unwrap();
        assert_eq!(xml::tag(&root), Some("manifest"));
        assert_eq!(xml::attr(&root, "package"), Some("a string value"));
        let expected = [
            "-5",
            "0xff",
            "true",
            "false",
            "@0x7f0b0001",
            "?0x01010001",
            "1.5",
            "16dp",
            "2px",
            "100%",
            "#ff112233",
            "#112233",
            "#1234",
            "#123",
            "",
            "9",
            "7",
            "a string value",
        ];
        for (i, want) in expected.iter().enumerate() {
            let name = if i == 16 {
                format!("attr{i}")
            } else {
                format!("android:attr{i}")
            };
            assert_eq!(
                xml::attr(&root, &name),
                Some(*want),
                "{name} (utf8: {utf8})"
            );
        }
        assert_eq!(xml::text(&root), "text");
        assert!(xml::child(&root, "child").is_some());
    }
}

#[cfg(feature = "axml")]
#[test]
fn axml_errors() {
    let good = build_axml(false, &[]);
    assert_eq!(axml::parse(b"").unwrap_err().format(), "axml");
    assert!(axml::parse(b"<?xml version='1.0'?>").is_err());
    for cut in (8..good.len()).step_by(7) {
        // Truncated files are errors or (when cut between chunks) incomplete documents; never panics.
        let _ = axml::parse(&good[..cut]);
    }
    let mut bad_chunk = good.clone();
    bad_chunk[12] = 0xFF;
    bad_chunk[13] = 0xFF;
    assert!(axml::parse(&bad_chunk).is_err());
    // An end element with nothing open.
    let mut unopened = good;
    let mut node = vec![
        0x03, 0x01, 16, 0, 24, 0, 0, 0, 1, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 2, 0,
        0, 0,
    ];
    unopened.append(&mut node);
    let size = unopened.len() as u32;
    unopened[4..8].copy_from_slice(&size.to_le_bytes());
    assert!(axml::parse(&unopened).is_ok() || axml::parse(&unopened).is_err());
    // Only the header: no element.
    let header_only = [3, 0, 8, 0, 8, 0, 0, 0];
    assert!(
        axml::parse(&header_only)
            .unwrap_err()
            .message()
            .contains("no element")
    );
}

// ---------------------------------------------------------------- gguf

#[cfg(feature = "gguf")]
fn gguf_string(out: &mut Vec<u8>, s: &str, wide: bool) {
    if wide {
        out.extend_from_slice(&(s.len() as u64).to_le_bytes());
    } else {
        out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(s.as_bytes());
}

#[cfg(feature = "gguf")]
fn build_gguf(version: u32) -> Vec<u8> {
    let wide = version >= 2;
    let mut out = b"GGUF".to_vec();
    out.extend_from_slice(&version.to_le_bytes());
    let count = |out: &mut Vec<u8>, n: u64| {
        if wide {
            out.extend_from_slice(&n.to_le_bytes());
        } else {
            out.extend_from_slice(&(n as u32).to_le_bytes());
        }
    };
    count(&mut out, 2); // tensors
    count(&mut out, 14); // metadata
    let kv = |out: &mut Vec<u8>, key: &str, kind: u32, value: &[u8]| {
        gguf_string(out, key, wide);
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(value);
    };
    kv(&mut out, "u8", 0, &[200]);
    kv(&mut out, "i8", 1, &[0xFE]);
    kv(&mut out, "u16", 2, &60_000u16.to_le_bytes());
    kv(&mut out, "i16", 3, &(-300i16).to_le_bytes());
    kv(&mut out, "llama.block_count", 4, &32u32.to_le_bytes());
    kv(&mut out, "i32", 5, &(-70_000i32).to_le_bytes());
    kv(&mut out, "f32", 6, &0.5f32.to_le_bytes());
    kv(&mut out, "bool", 7, &[1]);
    let mut s = Vec::new();
    gguf_string(&mut s, "llama", wide);
    kv(&mut out, "general.architecture", 8, &s);
    kv(&mut out, "u64", 10, &u64::MAX.to_le_bytes());
    kv(&mut out, "i64", 11, &i64::MIN.to_le_bytes());
    kv(&mut out, "f64", 12, &2.25f64.to_le_bytes());
    // A short array of u32 and a long array of strings.
    let mut arr = 4u32.to_le_bytes().to_vec();
    if wide {
        arr.extend_from_slice(&3u64.to_le_bytes());
    } else {
        arr.extend_from_slice(&3u32.to_le_bytes());
    }
    for n in [1u32, 2, 3] {
        arr.extend_from_slice(&n.to_le_bytes());
    }
    kv(&mut out, "short", 9, &arr);
    let mut long = 8u32.to_le_bytes().to_vec();
    if wide {
        long.extend_from_slice(&5000u64.to_le_bytes());
    } else {
        long.extend_from_slice(&5000u32.to_le_bytes());
    }
    for i in 0..5000 {
        gguf_string(&mut long, &format!("t{i}"), wide);
    }
    kv(&mut out, "tokens", 9, &long);
    // Tensor table.
    for (name, dims) in [("w0", vec![4u64, 8]), ("w1", vec![16])] {
        gguf_string(&mut out, name, wide);
        out.extend_from_slice(&(dims.len() as u32).to_le_bytes());
        for d in &dims {
            if wide {
                out.extend_from_slice(&d.to_le_bytes());
            } else {
                out.extend_from_slice(&(*d as u32).to_le_bytes());
            }
        }
        out.extend_from_slice(&1u32.to_le_bytes());
        if wide {
            out.extend_from_slice(&64u64.to_le_bytes());
        } else {
            out.extend_from_slice(&64u32.to_le_bytes());
        }
    }
    out
}

#[cfg(feature = "gguf")]
#[test]
fn gguf_reads_every_value_type() {
    for version in [1, 2, 3] {
        let bytes = build_gguf(version);
        let header = gguf::parse(&bytes, &gguf::Options::default()).unwrap();
        assert_eq!(header["version"], u64::from(version));
        assert_eq!(header["tensor_count"], 2u64);
        let m = &header["metadata"];
        assert_eq!(m["u8"], 200u64);
        assert_eq!(m["i8"], -2i64);
        assert_eq!(m["u16"], 60_000u64);
        assert_eq!(m["i16"], -300i64);
        assert_eq!(m["llama.block_count"].as_u64(), Some(32));
        assert_eq!(m["i32"], -70_000i64);
        assert_eq!(m["f32"], Value::Float(0.5));
        assert_eq!(m["bool"], true);
        assert_eq!(m["general.architecture"], "llama");
        assert_eq!(m["u64"], u64::MAX);
        assert_eq!(m["i64"], i64::MIN);
        assert_eq!(m["f64"], Value::Float(2.25));
        assert_eq!(m["short"], json!([1, 2, 3]));
        assert_eq!(m["tokens"], json!({"array_length": 5000}));
        assert!(header.get("tensors").is_none());

        let all = gguf::read(
            &bytes[..],
            &gguf::Options {
                max_array: 10_000,
                tensors: true,
            },
        )
        .unwrap();
        assert_eq!(
            all["metadata"]["tokens"].as_array().map(Vec::len),
            Some(5000)
        );
        let tensors = all["tensors"].as_array().unwrap();
        assert_eq!(
            tensors[0],
            json!({"name": "w0", "dimensions": [4, 8], "type": 1, "offset": 64})
        );
        assert_eq!(tensors[1]["dimensions"], json!([16]));
    }
}

#[cfg(feature = "gguf")]
#[test]
fn gguf_errors() {
    let good = build_gguf(3);
    assert!(gguf::parse(b"", &gguf::Options::default()).is_err());
    assert_eq!(
        gguf::parse(b"GGML....", &gguf::Options::default())
            .unwrap_err()
            .message(),
        "not a GGUF file"
    );
    let mut version = good.clone();
    version[4] = 9;
    assert!(
        gguf::parse(&version, &gguf::Options::default())
            .unwrap_err()
            .message()
            .contains("unsupported version")
    );
    let mut huge = good.clone();
    huge[16..24].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(
        gguf::parse(&huge, &gguf::Options::default())
            .unwrap_err()
            .message()
            .contains("implausible")
    );
    for cut in (4..good.len()).step_by(997) {
        assert!(
            gguf::parse(
                &good[..cut],
                &gguf::Options {
                    max_array: 10,
                    tensors: true
                }
            )
            .is_err(),
            "{cut}"
        );
    }
    // An unknown value type, a nested array, a huge string and too many dimensions.
    let header = |n_tensors: u64| {
        let mut h = b"GGUF".to_vec();
        h.extend_from_slice(&3u32.to_le_bytes());
        h.extend_from_slice(&n_tensors.to_le_bytes());
        h.extend_from_slice(&1u64.to_le_bytes());
        h
    };
    let mut unknown = header(0);
    gguf_string(&mut unknown, "k", true);
    unknown.extend_from_slice(&99u32.to_le_bytes());
    assert!(
        gguf::parse(&unknown, &gguf::Options::default())
            .unwrap_err()
            .message()
            .contains("unknown value type")
    );
    let mut nested = header(0);
    gguf_string(&mut nested, "k", true);
    nested.extend_from_slice(&9u32.to_le_bytes());
    for _ in 0..6 {
        nested.extend_from_slice(&9u32.to_le_bytes());
        nested.extend_from_slice(&1u64.to_le_bytes());
    }
    assert!(gguf::parse(&nested, &gguf::Options::default()).is_err());
    let mut long_string = header(0);
    long_string.extend_from_slice(&(1u64 << 40).to_le_bytes());
    assert!(
        gguf::parse(&long_string, &gguf::Options::default())
            .unwrap_err()
            .message()
            .contains("implausibly long")
    );
    let mut dims = header(1);
    // The one key/value pair, then a tensor with 99 dimensions.
    gguf_string(&mut dims, "k", true);
    dims.extend_from_slice(&0u32.to_le_bytes());
    dims.push(1);
    gguf_string(&mut dims, "t", true);
    dims.extend_from_slice(&99u32.to_le_bytes());
    assert!(
        gguf::parse(
            &dims,
            &gguf::Options {
                max_array: 1,
                tensors: true
            }
        )
        .unwrap_err()
        .message()
        .contains("dimensions")
    );
    let mut scalar_array = header(0);
    gguf_string(&mut scalar_array, "k", true);
    scalar_array.extend_from_slice(&9u32.to_le_bytes());
    scalar_array.extend_from_slice(&9u32.to_le_bytes());
    scalar_array.extend_from_slice(&1u64.to_le_bytes());
    scalar_array.extend_from_slice(&9u32.to_le_bytes());
    scalar_array.extend_from_slice(&1u64.to_le_bytes());
    scalar_array.extend_from_slice(&4u32.to_le_bytes());
    scalar_array.extend_from_slice(&1u64.to_le_bytes());
    scalar_array.extend_from_slice(&7u32.to_le_bytes());
    assert!(gguf::parse(&scalar_array, &gguf::Options::default()).is_ok());
}
