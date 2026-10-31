//! Writing: what we write is read back by us and by the stock `sqlite3`, and a crash never half-commits.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::*;

/// Runs the stock `sqlite3`; `None` when it is not installed (those checks are skipped, the rest still run).
fn sqlite3(path: &Path, sql: &str) -> Option<String> {
    let out = Command::new("sqlite3").arg(path).arg(sql).output().ok()?;
    assert!(
        out.status.success(),
        "sqlite3 failed on {sql}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn assert_integrity(path: &Path) {
    // Our own checker first: it sees what `PRAGMA integrity_check` of the stock tool cannot name for us.
    let found = reopen(path)
        .and_then(|mut db| db.integrity_check())
        .expect("the check runs");
    assert!(
        found.is_empty(),
        "our integrity check of {}: {found:?}",
        path.display()
    );
    if let Some(answer) = sqlite3(path, "PRAGMA integrity_check") {
        assert_eq!(
            answer,
            "ok",
            "sqlite3 integrity_check of {}",
            path.display()
        );
    }
}

/// Opens for writing, waiting out a `Busy` that lasts a moment: a test thread that starts `sqlite3` forks, and
/// for an instant the child holds copies of every open file (and their locks) until it calls `exec`.
fn reopen(path: &Path) -> Result<Database> {
    for _ in 0..200 {
        match Database::open_rw(path) {
            Err(Error::Busy) => std::thread::sleep(std::time::Duration::from_millis(5)),
            other => return other,
        }
    }
    Database::open_rw(path)
}

fn new_db(page_size: u32) -> (quench_core::tmp::TempDir, PathBuf, Database) {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("test.db");
    let db = Database::create(&path, page_size).expect("create");
    (dir, path, db)
}

/// A small deterministic generator (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

type Rows = BTreeMap<i64, Vec<Value>>;

fn row(id: i64, len: usize) -> Vec<Value> {
    vec![
        Value::Null,
        Value::Text(format!("name-{id}")),
        Value::Blob((0..len).map(|i| (i as i64 + id) as u8).collect()),
    ]
}

fn all_rows(db: &mut Database, root: u32) -> Rows {
    let mut out = BTreeMap::new();
    db.scan_table(root, &mut |id, values| {
        out.insert(id, values);
        Ok(())
    })
    .expect("scan");
    out
}

#[quench_test::test]
fn a_new_database_is_one_page_and_sqlite3_accepts_it() {
    let (_dir, path, db) = new_db(4096);
    drop(db);
    assert_eq!(std::fs::metadata(&path).expect("meta").len(), 4096);
    assert_integrity(&path);
    let mut db = Database::open(&path).expect("open");
    assert!(db.objects().expect("objects").is_empty());
    assert_eq!(db.header().page_size, 4096);
    for size in [512, 1024, 8192, 65_536] {
        let (_d, p, db) = new_db(size);
        drop(db);
        assert_integrity(&p);
    }
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    assert!(Database::create(dir.path().join("x.db"), 1000).is_err());
}

#[quench_test::test]
fn tables_made_here_are_read_by_sqlite3() {
    let (_dir, path, mut db) = new_db(1024);
    let root = db
        .write(|db| {
            let root = db.create_table(
                "items",
                "CREATE TABLE items(id INTEGER PRIMARY KEY, name TEXT, data BLOB)",
            )?;
            for id in 1..=300 {
                db.insert_row(root, id, &row(id, (id as usize * 7) % 3000))?;
            }
            Ok(root)
        })
        .expect("write");
    drop(db);
    assert_integrity(&path);
    if let Some(count) = sqlite3(
        &path,
        "select count(*), sum(length(data)), min(name), max(id) from items",
    ) {
        let expected: usize = (1..=300_usize).map(|id| (id * 7) % 3000).sum();
        assert_eq!(count, format!("300|{expected}|name-1|300"));
    }
    let mut db = Database::open(&path).expect("open");
    let rows = all_rows(&mut db, root);
    assert_eq!(rows.len(), 300);
    assert_eq!(rows[&299], row(299, 2093));
}

#[quench_test::test]
fn a_hundred_thousand_ascending_rows_and_a_descending_run_build_valid_trees() {
    let (_dir, path, mut db) = new_db(4096);
    let (up, down) = db
        .write(|db| {
            let up = db.create_table("up", "CREATE TABLE up(id INTEGER PRIMARY KEY, a, b)")?;
            let down =
                db.create_table("down", "CREATE TABLE down(id INTEGER PRIMARY KEY, a, b)")?;
            for id in 1..=20_000 {
                db.insert_row(up, id, &row(id, 20))?;
                db.insert_row(down, 20_001 - id, &row(20_001 - id, 20))?;
            }
            Ok((up, down))
        })
        .expect("write");
    drop(db);
    assert_integrity(&path);
    let mut db = Database::open(&path).expect("open");
    assert_eq!(all_rows(&mut db, up).len(), 20_000);
    assert_eq!(all_rows(&mut db, down), all_rows(&mut db, up));
    assert_eq!(
        db.get_row(down, 777).expect("get").expect("row"),
        row(777, 20)
    );
}

#[quench_test::test]
fn random_inserts_replaces_and_deletes_match_a_model_and_stay_valid() {
    for (page_size, seed) in [(512_u32, 1_u64), (1024, 2), (4096, 3)] {
        let (_dir, path, mut db) = new_db(page_size);
        let root = db
            .write(|db| db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a, b)"))
            .expect("create");
        let mut model: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        for round in 0..30 {
            db.write(|db| {
                for _ in 0..200 {
                    let id = rng.below(1500) as i64 + 1;
                    // Sizes from tiny to several pages, so cells spill to overflow pages and back.
                    let len = [0, 5, 60, 300, 2_000, 9_000][rng.below(6) as usize];
                    match rng.below(10) {
                        0..=4 => {
                            let r = row(id, len);
                            if let std::collections::btree_map::Entry::Vacant(slot) =
                                model.entry(id)
                            {
                                db.insert_row(root, id, &r)?;
                                slot.insert(r);
                            } else {
                                assert!(matches!(
                                    db.insert_row(root, id, &r),
                                    Err(Error::KeyExists)
                                ));
                            }
                        }
                        5..=6 => {
                            let r = row(id, len);
                            db.replace_row(root, id, &r)?;
                            model.insert(id, r);
                        }
                        _ => {
                            assert_eq!(db.delete_row(root, id)?, model.remove(&id).is_some());
                        }
                    }
                }
                Ok(())
            })
            .expect("write");
            if round % 5 == 4 {
                assert_eq!(
                    all_rows(&mut db, root),
                    model,
                    "page size {page_size}, round {round}"
                );
            }
        }
        drop(db);
        assert_integrity(&path);
        let mut db = Database::open(&path).expect("open");
        assert_eq!(all_rows(&mut db, root), model);
        for (id, r) in model.iter().step_by(13) {
            assert_eq!(&db.get_row(root, *id).expect("get").expect("row"), r);
        }
    }
}

#[quench_test::test]
fn deleting_everything_gives_the_pages_back_and_they_are_reused() {
    let (_dir, path, mut db) = new_db(1024);
    let root = db
        .write(|db| {
            let root = db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a, b)")?;
            for id in 1..=2_000 {
                db.insert_row(root, id, &row(id, 100))?;
            }
            Ok(root)
        })
        .expect("fill");
    let full = db.header().page_count;
    db.write(|db| {
        for id in 1..=2_000 {
            assert!(db.delete_row(root, id)?);
        }
        Ok(())
    })
    .expect("empty");
    assert!(all_rows(&mut db, root).is_empty());
    assert!(
        db.header().freelist_count > full / 2,
        "most pages are free: {}",
        db.header().freelist_count
    );
    db.write(|db| {
        for id in 1..=2_000 {
            db.insert_row(root, id, &row(id, 100))?;
        }
        Ok(())
    })
    .expect("refill");
    assert!(
        db.header().page_count <= full + 2,
        "the freed pages were reused"
    );
    drop(db);
    assert_integrity(&path);
}

#[quench_test::test]
fn dropping_a_table_frees_its_pages_and_the_schema_cookie_changes() {
    let (_dir, path, mut db) = new_db(1024);
    db.write(|db| {
        let keep = db.create_table("keep", "CREATE TABLE keep(id INTEGER PRIMARY KEY, a, b)")?;
        db.insert_row(keep, 1, &row(1, 10))?;
        let gone = db.create_table("gone", "CREATE TABLE gone(id INTEGER PRIMARY KEY, a, b)")?;
        for id in 1..=500 {
            db.insert_row(gone, id, &row(id, 300))?;
        }
        Ok(())
    })
    .expect("create");
    let cookie = db.header().schema_cookie;
    assert!(
        db.write(|db| db.drop_object("GONE")).expect("drop"),
        "names are not case sensitive"
    );
    assert!(!db.write(|db| db.drop_object("gone")).expect("drop again"));
    assert!(db.header().schema_cookie > cookie);
    assert!(db.header().freelist_count > 100);
    let names: Vec<_> = db
        .tables()
        .expect("tables")
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(names, ["keep"]);
    drop(db);
    assert_integrity(&path);
    if let Some(tables) = sqlite3(&path, "select name from sqlite_schema") {
        assert_eq!(tables, "keep");
    }
}

#[quench_test::test]
fn an_index_made_here_agrees_with_its_table_for_sqlite3() {
    let (_dir, path, mut db) = new_db(1024);
    let mut rng = Rng(42);
    let mut model: BTreeMap<i64, String> = BTreeMap::new();
    let (table, index) = db
        .write(|db| {
            let t = db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT)")?;
            let i = db.create_index("t_a", "t", "CREATE INDEX t_a ON t(a)")?;
            Ok((t, i))
        })
        .expect("create");
    for _ in 0..12 {
        db.write(|db| {
            for _ in 0..150 {
                let id = rng.below(1200) as i64 + 1;
                // Long keys now and then, so index cells spill to overflow pages.
                let text = format!(
                    "{:0width$}",
                    rng.below(5000),
                    width = [3, 40, 700][rng.below(3) as usize]
                );
                if let Some(old) = model.get(&id).cloned() {
                    assert!(db.index_delete(index, &[Value::Text(old), Value::Integer(id)])?);
                    assert!(db.delete_row(table, id)?);
                    model.remove(&id);
                } else {
                    db.insert_row(table, id, &[Value::Null, Value::Text(text.clone())])?;
                    db.index_insert(index, &[Value::Text(text.clone()), Value::Integer(id)])?;
                    model.insert(id, text);
                }
            }
            Ok(())
        })
        .expect("write");
    }
    drop(db);
    // sqlite3 checks every index entry against its row, and every row against the index.
    assert_integrity(&path);
    let mut db = Database::open(&path).expect("open");
    let mut entries = Vec::new();
    db.scan_index(index, &mut |e| {
        entries.push(e);
        Ok(())
    })
    .expect("scan");
    let mut expected: Vec<(String, i64)> = model.iter().map(|(id, t)| (t.clone(), *id)).collect();
    expected.sort();
    let got: Vec<(String, i64)> = entries
        .iter()
        .map(|e| {
            (
                e[0].as_text().expect("text").to_string(),
                e[1].as_integer().expect("rowid"),
            )
        })
        .collect();
    assert_eq!(got, expected);
}

#[quench_test::test]
fn an_index_survives_deleting_down_to_nothing_and_filling_again() {
    let (_dir, path, mut db) = new_db(512);
    let index = db
        .write(|db| {
            db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a)")?;
            db.create_index("t_a", "t", "CREATE INDEX t_a ON t(a)")
        })
        .expect("create");
    let entry = |i: i64| vec![Value::Integer(i * 7 % 1013), Value::Integer(i)];
    for round in 0..2 {
        db.write(|db| {
            for i in 1..=1500 {
                db.index_insert(index, &entry(i))?;
            }
            Ok(())
        })
        .expect("fill");
        // Out of order, so that interior entries go too.
        let mut order: Vec<i64> = (1..=1500).collect();
        let mut rng = Rng(7 + round);
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i as u64 + 1) as usize);
        }
        db.write(|db| {
            for (n, i) in order.iter().enumerate() {
                assert!(db.index_delete(index, &entry(*i))?);
                if n == 700 {
                    let mut left = 0;
                    db.scan_index(index, &mut |_| {
                        left += 1;
                        Ok(())
                    })?;
                    assert_eq!(left, 1500 - 701);
                }
            }
            assert!(!db.index_delete(index, &entry(1))?);
            Ok(())
        })
        .expect("empty");
    }
    drop(db);
    // The table is empty, so an index with entries would be reported; this one has none.
    assert_integrity(&path);
}

#[quench_test::test]
fn duplicates_are_refused_and_a_failed_write_leaves_nothing_behind() {
    let (_dir, path, mut db) = new_db(1024);
    let root = db
        .write(|db| {
            let r = db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a, b)")?;
            db.insert_row(r, 1, &row(1, 10))?;
            Ok(r)
        })
        .expect("create");
    let before = std::fs::read(&path).expect("read");
    let err = db
        .write(|db| {
            db.insert_row(root, 2, &row(2, 5000))?;
            db.insert_row(root, 1, &row(1, 10))
        })
        .expect_err("duplicate");
    assert!(matches!(err, Error::KeyExists));
    assert_eq!(
        std::fs::read(&path).expect("read"),
        before,
        "the file was not touched"
    );
    assert!(
        db.get_row(root, 2).expect("get").is_none(),
        "rolled back in memory too"
    );
    assert!(
        db.insert_row(root, 3, &row(3, 1)).is_err(),
        "no changes outside a transaction"
    );
    assert!(matches!(
        db.create_table("t", "CREATE TABLE t(x)"),
        Err(Error::ReadOnly(_) | Error::KeyExists)
    ));
}

#[quench_test::test]
fn a_database_open_for_reading_cannot_be_written_and_one_writer_locks_the_file() {
    let (_dir, path, mut db) = new_db(1024);
    db.write(|db| db.create_table("t", "CREATE TABLE t(a)").map(|_| ()))
        .expect("create");
    assert!(
        matches!(Database::open_rw(&path), Err(Error::Busy)),
        "a second writer is refused"
    );
    let mut reader = Database::open(&path).expect("readers are fine");
    assert!(matches!(reader.begin(), Err(Error::ReadOnly(_))));
    drop(db);
    assert!(reopen(&path).is_ok(), "free again once the writer is gone");
}

/// The state of a database as a comparable value: the rows of its first table. Opening for writing is what
/// recovers a hot journal, so this is what a program does after a crash.
fn state(path: &Path) -> Rows {
    let mut db = reopen(path).expect("open after the crash");
    let root = db
        .tables()
        .expect("tables")
        .first()
        .map_or(0, |t| t.root_page);
    if root == 0 {
        return BTreeMap::new();
    }
    all_rows(&mut db, root)
}

#[quench_test::test]
fn a_crash_at_every_write_step_leaves_the_old_state_or_the_new_never_a_mix() {
    // The transaction changes many pages: new rows with overflow, replaced rows, deleted rows, splits.
    let build = |path: &Path| -> (Rows, Rows, u32) {
        let mut db = Database::create(path, 512).expect("create");
        let root = db
            .write(|db| {
                let r = db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a, b)")?;
                for id in 1..=150 {
                    db.insert_row(r, id, &row(id, 40))?;
                }
                Ok(r)
            })
            .expect("seed");
        let old = all_rows(&mut db, root);
        drop(db);
        let mut new = old.clone();
        for id in 151..=260 {
            new.insert(id, row(id, 900));
        }
        for id in (1..=150).step_by(3) {
            new.remove(&id);
        }
        for id in (2..=150).step_by(5) {
            new.insert(id, row(id, 1500));
        }
        (old, new, root)
    };
    let apply = |db: &mut Database, root: u32, new: &Rows| -> Result<()> {
        let current = all_rows(db, root);
        for id in current.keys().filter(|id| !new.contains_key(id)) {
            db.delete_row(root, *id)?;
        }
        for (id, r) in new {
            db.replace_row(root, *id, r)?;
        }
        Ok(())
    };

    // How many write steps does a clean commit take? Count with a failure point far away.
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let probe = dir.path().join("probe.db");
    let (_, new_state, root) = build(&probe);
    let mut db = reopen(&probe).expect("open");
    db.begin().expect("begin");
    apply(&mut db, root, &new_state).expect("apply");
    db.pager.fail_at = Some(usize::MAX);
    db.commit().expect("clean commit");
    let steps = usize::MAX - db.pager.fail_at.expect("counter");
    drop(db);
    assert!(
        steps > 50,
        "the transaction is big enough to be interesting: {steps} steps"
    );

    for k in 1..=steps {
        let path = dir.path().join(format!("crash-{k}.db"));
        let (old, new, root) = build(&path);
        let mut db = reopen(&path).expect("open");
        db.begin().expect("begin");
        apply(&mut db, root, &new).expect("apply");
        db.pager.fail_at = Some(k);
        let failed = db.commit();
        drop(db);
        let after = state(&path);
        if failed.is_ok() {
            assert_eq!(after, new, "step {k}: a commit that returned Ok is there");
        } else {
            assert!(
                after == old || after == new,
                "step {k}: neither the old state nor the new"
            );
        }
        // Whatever was left, opening for writing recovered it, and sqlite3 finds nothing wrong.
        assert!(
            !PathBuf::from(format!("{}-journal", path.display())).exists(),
            "step {k}: journal left"
        );
        assert_integrity(&path);
        assert_eq!(state(&path), after, "step {k}: recovery changed the state");
    }
}

#[quench_test::test]
fn a_hot_journal_blocks_readers_and_sqlite3_can_recover_one_of_ours() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("hot.db");
    let mut db = Database::create(&path, 1024).expect("create");
    let root = db
        .write(|db| {
            let r = db.create_table("t", "CREATE TABLE t(id INTEGER PRIMARY KEY, a, b)")?;
            for id in 1..=100 {
                db.insert_row(r, id, &row(id, 10))?;
            }
            Ok(r)
        })
        .expect("seed");
    db.begin().expect("begin");
    for id in 101..=400 {
        db.insert_row(root, id, &row(id, 10)).expect("insert");
    }
    // Crash after the database pages were written but before the journal was removed: the last step.
    let steps = {
        let probe = dir.path().join("probe.db");
        std::fs::copy(&path, &probe).expect("copy");
        let mut other = reopen(&probe).expect("open");
        other.begin().expect("begin");
        for id in 101..=400 {
            other.insert_row(root, id, &row(id, 10)).expect("insert");
        }
        other.pager.fail_at = Some(usize::MAX);
        other.commit().expect("clean commit");
        usize::MAX - other.pager.fail_at.expect("counter")
    };
    db.pager.fail_at = Some(steps);
    assert!(db.commit().is_err());
    drop(db);
    let journal = format!("{}-journal", path.display());
    assert!(PathBuf::from(&journal).exists(), "the crash left a journal");
    assert!(
        Database::open(&path).is_err(),
        "readers do not look at a half-written file"
    );
    // The stock sqlite3 rolls it back by itself.
    if let Some(count) = sqlite3(&path, "select count(*) from t") {
        assert!(
            count == "100" || count == "400",
            "sqlite3 sees {count} rows"
        );
    }
}
