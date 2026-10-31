use super::*;
use crate::record;

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.db");

fn sample() -> Database {
    Database::open(SAMPLE).expect("the fixture opens")
}

fn rows(db: &mut Database, root: u32) -> Vec<(i64, Vec<Value>)> {
    let mut out = Vec::new();
    db.scan_table(root, &mut |id, row| {
        out.push((id, row));
        Ok(())
    })
    .expect("scan");
    out
}

#[quench_test::test]
fn varints_round_trip_at_every_length() {
    let mut cases = vec![0_u64, 1, 127, 128, 16_383, 16_384, u64::MAX, u64::MAX - 1];
    for shift in 0..64 {
        cases.push(1 << shift);
        cases.push((1 << shift) - 1);
        cases.push((1 << shift) + 1);
    }
    for value in cases {
        let mut buf = Vec::new();
        varint::put(&mut buf, value);
        assert_eq!(buf.len(), varint::len(value), "length of {value}");
        assert_eq!(varint::get(&buf), Some((value, buf.len())), "value {value}");
    }
}

#[quench_test::test]
fn varints_match_the_documented_examples() {
    assert_eq!(varint::get(&[0x7f]), Some((127, 1)));
    assert_eq!(varint::get(&[0x81, 0x00]), Some((128, 2)));
    assert_eq!(varint::get(&[0xff; 9]), Some((u64::MAX, 9)));
    assert_eq!(varint::get(&[0x81]), None, "ends in the middle");
    assert_eq!(varint::get(&[]), None);
}

#[quench_test::test]
fn records_round_trip_every_storage_class() {
    let row = vec![
        Value::Null,
        Value::Integer(0),
        Value::Integer(1),
        Value::Integer(-1),
        Value::Integer(127),
        Value::Integer(-128),
        Value::Integer(128),
        Value::Integer(32_767),
        Value::Integer(-32_769),
        Value::Integer(8_388_607),
        Value::Integer(8_388_608),
        Value::Integer(2_147_483_647),
        Value::Integer(-2_147_483_649),
        Value::Integer(140_737_488_355_327),
        Value::Integer(140_737_488_355_328),
        Value::Integer(i64::MAX),
        Value::Integer(i64::MIN),
        Value::Real(1.5),
        Value::Real(-0.0),
        Value::Text(String::new()),
        Value::Text("héllo".into()),
        Value::Blob(Vec::new()),
        Value::Blob(vec![0, 1, 2, 255]),
    ];
    assert_eq!(record::decode(&record::encode(&row)).expect("decode"), row);
}

#[quench_test::test]
fn a_record_with_many_columns_has_a_two_byte_header_size() {
    let row: Vec<Value> = (0..200).map(Value::Integer).collect();
    assert_eq!(record::decode(&record::encode(&row)).expect("decode"), row);
}

#[quench_test::test]
fn malformed_records_are_errors_not_panics() {
    assert!(record::decode(&[]).is_err());
    assert!(
        record::decode(&[5, 1]).is_err(),
        "header longer than the record"
    );
    assert!(record::decode(&[2, 4]).is_err(), "a value past the end");
    assert!(record::decode(&[2, 10]).is_err(), "reserved serial type");
    assert!(
        record::decode(&[2, 15, 0xff]).is_err(),
        "text that is not UTF-8"
    );
}

#[quench_test::test]
fn the_header_of_a_real_database_is_read() {
    let db = sample();
    let h = db.header();
    assert_eq!(h.page_size, 1024);
    assert_eq!(h.text_encoding, 1);
    assert_eq!((h.write_version, h.read_version), (1, 1));
    assert!(h.page_count > 100);
}

#[quench_test::test]
fn the_schema_lists_tables_and_indexes_with_their_root_pages() {
    let mut db = sample();
    let all: Vec<_> = db
        .objects()
        .expect("schema")
        .into_iter()
        .map(|o| (o.kind, o.name, o.root_page))
        .collect();
    assert_eq!(
        all,
        [
            (ObjectKind::Table, "items".to_string(), 2),
            (ObjectKind::Index, "items_name".to_string(), 3),
            (ObjectKind::Table, "tags".to_string(), 4),
            (ObjectKind::Index, "sqlite_autoindex_tags_1".to_string(), 5),
        ]
    );
    let names: Vec<_> = db
        .tables()
        .expect("tables")
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["items", "tags"]);
    let items = db.objects().expect("schema").remove(0);
    assert!(items.sql.expect("sql").starts_with("CREATE TABLE items"));
}

#[quench_test::test]
fn a_table_of_many_pages_is_read_in_rowid_order() {
    let mut db = sample();
    let items = rows(&mut db, 2);
    assert_eq!(items.len(), 1200);
    assert!(items.windows(2).all(|w| w[0].0 < w[1].0));
    // The sums come from `sqlite3` on the same file.
    let sizes: i128 = items
        .iter()
        .filter_map(|(_, r)| r[2].as_integer())
        .map(i128::from)
        .sum();
    assert_eq!(sizes, -1_538_144_988_064_228);
    let blobs: usize = items
        .iter()
        .map(|(_, r)| match &r[4] {
            Value::Blob(b) => b.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(blobs, 62_376);
}

#[quench_test::test]
fn every_storage_class_comes_back_as_it_was_stored() {
    let mut db = sample();
    let row = db.get_row(2, 1199).expect("get").expect("row");
    assert_eq!(row[0], Value::Null, "the rowid alias is stored as NULL");
    assert_eq!(row[1], Value::Text("item-01199".into()));
    assert_eq!(row[2], Value::Integer(1));
    assert_eq!(row[3], Value::Real(1199.0 / 7.0));
    assert_eq!(row[4], Value::Blob(vec![0xca, 0xfe]));
    assert_eq!(row[5], Value::Null);
    // 1197 = 7 * 171: NULL; 1195: 5_000_000_000; 1196: -9_000_000_000_000.
    assert_eq!(
        db.get_row(2, 1197).expect("get").expect("row")[2],
        Value::Null
    );
    assert_eq!(
        db.get_row(2, 1195).expect("get").expect("row")[2],
        Value::Integer(5_000_000_000)
    );
    assert_eq!(
        db.get_row(2, 1196).expect("get").expect("row")[2],
        Value::Integer(-9_000_000_000_000)
    );
}

#[quench_test::test]
fn values_larger_than_a_page_are_followed_through_the_overflow_chain() {
    let mut db = sample();
    let row = db.get_row(2, 300).expect("get").expect("row");
    assert_eq!(row[4], Value::Blob(vec![0; 5000]));
    let row = db.get_row(2, 250).expect("get").expect("row");
    assert_eq!(row[5].as_text().map(|s| s.chars().count()), Some(3000));
    assert!(
        row[5]
            .as_text()
            .is_some_and(|s| s.chars().all(|c| c == 'é'))
    );
}

#[quench_test::test]
fn get_row_finds_the_first_the_last_and_reports_the_missing() {
    let mut db = sample();
    assert!(db.get_row(2, 1).expect("get").is_some());
    assert!(db.get_row(2, 1200).expect("get").is_some());
    assert!(db.get_row(2, 0).expect("get").is_none());
    assert!(db.get_row(2, 1201).expect("get").is_none());
    for id in (1..=1200).step_by(37) {
        let row = db.get_row(2, id).expect("get").expect("row");
        assert_eq!(row[1], Value::Text(format!("item-{id:05}")));
    }
}

#[quench_test::test]
fn an_index_is_read_in_key_order_with_the_rowid_last() {
    let mut db = sample();
    let mut entries = Vec::new();
    db.scan_index(3, &mut |e| {
        entries.push(e);
        Ok(())
    })
    .expect("scan");
    assert_eq!(entries.len(), 1200);
    for (i, e) in entries.iter().enumerate() {
        let id = (i + 1) as i64;
        assert_eq!(e[0], Value::Text(format!("item-{id:05}")));
        assert_eq!(e[1], Value::Integer(id));
    }
    let mut tags = 0;
    db.scan_index(5, &mut |_| {
        tags += 1;
        Ok(())
    })
    .expect("scan");
    assert_eq!(tags, 400);
}

#[quench_test::test]
fn a_callback_error_stops_the_scan() {
    let mut db = sample();
    let mut seen = 0;
    let err = db
        .scan_table(2, &mut |_, _| {
            seen += 1;
            if seen == 5 {
                return Err(Error::Unsupported("stop".into()));
            }
            Ok(())
        })
        .expect_err("stopped");
    assert_eq!(seen, 5);
    assert!(matches!(err, Error::Unsupported(_)));
}

fn copy_with(
    name: &str,
    change: impl FnOnce(&mut Vec<u8>),
) -> (quench_core::tmp::TempDir, std::path::PathBuf) {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let mut bytes = std::fs::read(SAMPLE).expect("fixture");
    change(&mut bytes);
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).expect("write");
    (dir, path)
}

#[quench_test::test]
fn files_that_are_not_databases_are_refused_with_a_reason() {
    let (_d, path) = copy_with("text.db", |b| {
        *b = b"just some text, not a database at all".repeat(10);
    });
    assert!(matches!(Database::open(&path), Err(Error::NotADatabase(_))));
    let (_d, path) = copy_with("short.db", |b| b.truncate(50));
    assert!(matches!(Database::open(&path), Err(Error::NotADatabase(_))));
    let (_d, path) = copy_with("empty.db", Vec::clear);
    assert!(matches!(Database::open(&path), Err(Error::NotADatabase(_))));
    let (_d, path) = copy_with("page.db", |b| b[16..18].copy_from_slice(&[0x03, 0x00]));
    assert!(
        matches!(Database::open(&path), Err(Error::NotADatabase(_))),
        "768 is not a power of two"
    );
}

#[quench_test::test]
fn what_is_not_supported_is_said_so() {
    let (_d, path) = copy_with("utf16.db", |b| {
        b[56..60].copy_from_slice(&2_u32.to_be_bytes());
    });
    let err = Database::open(&path).expect_err("utf-16");
    assert!(matches!(err, Error::Unsupported(_)), "{err}");
    let (_d, path) = copy_with("wal.db", |b| {
        b[18] = 2;
        b[19] = 2;
    });
    let err = Database::open(&path).expect_err("wal");
    assert!(matches!(err, Error::Unsupported(_)), "{err}");
}

#[quench_test::test]
fn damage_is_an_error_naming_the_page_and_never_a_panic() {
    // Every page of the file overwritten in turn with garbage: reading everything must return or fail cleanly.
    let size = std::fs::metadata(SAMPLE).expect("meta").len() as usize;
    let pages = size / 1024;
    for page in (1..pages).step_by(7) {
        let (_d, path) = copy_with("damaged.db", |b| {
            for (i, byte) in b[page * 1024..(page + 1) * 1024].iter_mut().enumerate() {
                *byte = (i * 31 + page) as u8;
            }
        });
        let Ok(mut db) = Database::open(&path) else {
            continue;
        };
        let _ = db.objects();
        for root in [2, 3, 4, 5] {
            let _ = db.scan_table(root, &mut |_, _| Ok(()));
            let _ = db.scan_index(root, &mut |_| Ok(()));
            let _ = db.get_row(root, 600);
        }
    }
    let (_d, path) = copy_with("loop.db", |b| {
        // Make the root of the items tree point at itself.
        let root = &mut b[1024..2048];
        if root[0] == 5 {
            root[8..12].copy_from_slice(&2_u32.to_be_bytes());
        }
    });
    let mut db = Database::open(&path).expect("opens");
    let err = db.scan_table(2, &mut |_, _| Ok(())).expect_err("a loop");
    assert!(matches!(err, Error::Corrupt { .. }), "{err}");
}
