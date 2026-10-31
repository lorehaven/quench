//! The integrity checker: sound databases pass, broken ones are named, and damage never panics anything.

use std::path::Path;

use crate::{Connection, Database, Value};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sample.db");

fn reopen_db(path: &Path) -> Database {
    for _ in 0..200 {
        match Database::open_rw(path) {
            Err(crate::Error::Busy) => std::thread::sleep(std::time::Duration::from_millis(5)),
            other => return other.expect("open"),
        }
    }
    Database::open_rw(path).expect("open")
}

fn problems(path: &Path) -> Vec<String> {
    reopen_db(path)
        .integrity_check()
        .expect("the check itself runs")
}

/// A database with a table, two indexes (one automatic) and enough rows for several levels, made by SQL.
fn built(path: &Path, rows: i64) {
    let mut c = Connection::create(path).expect("create");
    c.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, k TEXT UNIQUE, grp INTEGER, v TEXT)",
        &[],
    )
    .expect("table");
    c.execute("CREATE INDEX t_grp ON t (grp)", &[])
        .expect("index");
    c.transaction(|c| {
        for i in 1..=rows {
            c.execute(
                "INSERT INTO t (k, grp, v) VALUES (?, ?, ?)",
                &[
                    Value::Text(format!("key{i:05}")),
                    Value::Integer(i % 50),
                    Value::Text("v".repeat((i % 700) as usize)),
                ],
            )?;
        }
        Ok(())
    })
    .expect("fill");
    c.execute("DELETE FROM t WHERE grp = 3 OR id % 7 = 0", &[])
        .expect("delete");
    c.execute(
        "UPDATE t SET grp = grp + 100, v = v || 'x' WHERE id % 11 = 0",
        &[],
    )
    .expect("update");
}

#[quench_test::test]
fn databases_from_sqlite3_and_from_here_pass() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let copy = dir.path().join("sample.db");
    std::fs::copy(SAMPLE, &copy).expect("copy");
    assert_eq!(
        problems(&copy),
        Vec::<String>::new(),
        "a database made by the stock sqlite3"
    );
    let mine = dir.path().join("mine.db");
    built(&mine, 3000);
    assert_eq!(
        problems(&mine),
        Vec::<String>::new(),
        "a database made by SQL here, after deletes and updates"
    );
    // The same answer through the pragma.
    let mut c = Connection::open(&mine).expect("open");
    let r = c.query("PRAGMA integrity_check", &[]).expect("pragma");
    assert_eq!(r.rows, vec![vec![Value::Text("ok".into())]]);
}

#[quench_test::test]
fn an_index_that_disagrees_with_its_table_is_named() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("m.db");
    built(&path, 3000);
    // Take one entry out of the index but leave its row.
    let mut db = reopen_db(&path);
    let index = db
        .objects()
        .expect("objects")
        .into_iter()
        .find(|o| o.name == "t_grp")
        .expect("index")
        .root_page;
    db.begin().expect("begin");
    assert!(
        db.index_delete(index, &[Value::Integer(1), Value::Integer(1)])
            .expect("delete")
    );
    db.commit().expect("commit");
    drop(db);
    let found = problems(&path);
    assert!(found.iter().any(|p| p.contains("t_grp")), "{found:?}");
    // Take a row out and leave its entries.
    let mut db = reopen_db(&path);
    let table = db
        .objects()
        .expect("objects")
        .into_iter()
        .find(|o| o.name == "t")
        .expect("table")
        .root_page;
    db.begin().expect("begin");
    assert!(db.delete_row(table, 2).expect("delete"));
    db.commit().expect("commit");
    drop(db);
    let found = problems(&path);
    assert!(
        found.iter().any(|p| p.contains("wrong # of entries")),
        "{found:?}"
    );
}

#[quench_test::test]
fn pages_used_twice_or_not_at_all_and_a_wrong_freelist_count_are_named() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("p.db");
    built(&path, 1500);
    let good = std::fs::read(&path).expect("read");
    let size = u32::from(u16::from_be_bytes([good[16], good[17]]));
    // A page nothing points to: one more page at the end of the file, counted in the header.
    let mut grown = good.clone();
    grown.extend(vec![0_u8; size as usize]);
    let pages = u32::from_be_bytes([good[28], good[29], good[30], good[31]]) + 1;
    grown[28..32].copy_from_slice(&pages.to_be_bytes());
    std::fs::write(&path, &grown).expect("write");
    let found = problems(&path);
    assert!(found.iter().any(|p| p.contains("never used")), "{found:?}");
    // The freelist count in the header says more than the freelist holds.
    let mut wrong = good;
    let count = u32::from_be_bytes([wrong[36], wrong[37], wrong[38], wrong[39]]) + 5;
    wrong[36..40].copy_from_slice(&count.to_be_bytes());
    std::fs::write(&path, &wrong).expect("write");
    let found = problems(&path);
    assert!(found.iter().any(|p| p.contains("freelist")), "{found:?}");
}

#[quench_test::test]
fn random_damage_is_reported_or_refused_and_never_panics() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("f.db");
    built(&path, 400);
    let good = std::fs::read(&path).expect("read");
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut caught = 0;
    for round in 0..150 {
        let mut bytes = good.clone();
        for _ in 0..=(round % 4) {
            let at = (next() % bytes.len() as u64) as usize;
            bytes[at] = (next() & 0xff) as u8;
        }
        let damaged = dir.path().join("damaged.db");
        std::fs::write(&damaged, &bytes).expect("write");
        let Ok(mut db) = Database::open(&damaged) else {
            caught += 1;
            continue;
        };
        match db.integrity_check() {
            Ok(list) if !list.is_empty() => caught += 1,
            Err(_) => caught += 1,
            Ok(_) => {}
        }
        // Whatever the check said, reading must be safe.
        for table in db.tables().unwrap_or_default() {
            let _ = db.scan_table(table.root_page, &mut |_, _| Ok(()));
        }
        drop(db);
        if let Ok(mut c) = Connection::open(&damaged) {
            let _ = c.query(
                "SELECT count(*), sum(grp), max(v) FROM t WHERE grp > 5 GROUP BY grp ORDER BY 1",
                &[],
            );
            let _ = c.query("SELECT * FROM t WHERE k = 'key00500'", &[]);
            let _ = c.execute("UPDATE t SET v = 'z' WHERE id < 20", &[]);
            let _ = c.execute("DELETE FROM t WHERE grp = 7", &[]);
            let _ = c.execute("INSERT INTO t (k, grp, v) VALUES ('new', 1, 'x')", &[]);
        }
    }
    // Most single-byte changes land somewhere that matters; a few land in slack space and change nothing.
    assert!(
        caught > 40,
        "only {caught} of 150 damaged databases were noticed"
    );
}
