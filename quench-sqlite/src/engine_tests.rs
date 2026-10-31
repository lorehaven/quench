//! The SQL engine against the stock `sqlite3`: the same script through both, every query's output compared.

use std::process::Command;

use crate::{Connection, Error, Value};

fn format_value(v: &Value) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Integer(i) => i.to_string(),
        Value::Real(r) => crate::engine::value::format_real(*r),
        Value::Text(s) => s.clone(),
        Value::Blob(b) => String::from_utf8_lossy(b).into_owned(),
    }
}

/// Runs `statements` in `sqlite3`; the output of the queries, one line per row.
fn reference(statements: &[String]) -> Option<String> {
    let script = statements
        .iter()
        .map(|s| format!("{s};"))
        .collect::<Vec<_>>()
        .join("\n");
    let out = Command::new("sqlite3")
        .args(["-nullvalue", "NULL", "-separator", "|", ":memory:"])
        .arg(script)
        .output()
        .ok()?;
    assert!(
        out.stderr.is_empty(),
        "sqlite3 complained: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Runs `statements` here; the output of the queries in the same form.
fn ours(statements: &[String]) -> String {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let mut conn = Connection::create(dir.path().join("t.db")).expect("create");
    let mut out = String::new();
    for s in statements {
        let rows = conn
            .query(s, &[])
            .unwrap_or_else(|e| panic!("{s}\n  -> {e}"));
        for row in rows.rows {
            let line: Vec<String> = row.iter().map(format_value).collect();
            out.push_str(&line.join("|"));
            out.push('\n');
        }
    }
    out
}

fn check(statements: &[String]) {
    let Some(expected) = reference(statements) else {
        // No sqlite3 here: still run it, to see that nothing fails.
        let _ = ours(statements);
        return;
    };
    let got = ours(statements);
    if got != expected {
        let (g, e): (Vec<&str>, Vec<&str>) = (got.lines().collect(), expected.lines().collect());
        let at = g
            .iter()
            .zip(&e)
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| g.len().min(e.len()));
        panic!(
            "output differs at line {at}:\n  ours: {:?}\n  sqlite3: {:?}\n({} lines ours, {} lines sqlite3)",
            g.get(at),
            e.get(at),
            g.len(),
            e.len()
        );
    }
}

fn s(x: &str) -> String {
    x.to_string()
}

/// Forklift's manifest, with data.
fn manifest() -> Vec<String> {
    let mut v = vec![
        s(
            "CREATE TABLE items (category TEXT NOT NULL, local_id TEXT NOT NULL, name TEXT, size INTEGER, \
           modified_ms INTEGER, status TEXT NOT NULL DEFAULT 'pending', attempts INTEGER NOT NULL DEFAULT 0, \
           retry_at_ms INTEGER NOT NULL DEFAULT 0, error TEXT, PRIMARY KEY (category, local_id))",
        ),
        s("CREATE INDEX items_status ON items (status, retry_at_ms)"),
    ];
    let statuses = ["pending", "done", "failed", "uploading", "too_large"];
    for i in 1..=600_i64 {
        let cat = ["photo", "video", "doc"][(i % 3) as usize];
        let status = statuses[(i % 5) as usize];
        let size = if i % 11 == 0 {
            "NULL".to_string()
        } else {
            (i * 7919 % 100_000).to_string()
        };
        let err = if status == "failed" {
            format!("'error {i}'")
        } else {
            "NULL".into()
        };
        v.push(format!(
            "INSERT INTO items (category, local_id, name, size, modified_ms, status, attempts, retry_at_ms, error) \
             VALUES ('{cat}', 'id{i:04}', 'file-{i}.jpg', {size}, {}, '{status}', {}, {}, {err})",
            1_700_000_000_000_i64 + i * 1000,
            i % 4,
            (i % 7) * 1000
        ));
    }
    v
}

fn with(base: Vec<String>, queries: &[&str]) -> Vec<String> {
    base.into_iter()
        .chain(queries.iter().map(|q| s(q)))
        .collect()
}

#[quench_test::test]
fn selects_over_the_manifest_match_sqlite3() {
    check(&with(
        manifest(),
        &[
            "SELECT count(*) FROM items",
            "SELECT * FROM items WHERE category = 'photo' AND local_id = 'id0003'",
            "SELECT local_id, name FROM items WHERE category = 'video' AND local_id = 'id0004'",
            "SELECT local_id FROM items WHERE status = 'pending' AND retry_at_ms <= 3000 ORDER BY local_id LIMIT 7",
            "SELECT local_id FROM items WHERE status = 'pending' AND retry_at_ms > 3000 ORDER BY retry_at_ms, local_id LIMIT 5 OFFSET 2",
            "SELECT min(retry_at_ms) FROM items WHERE status = 'pending' AND retry_at_ms > 1000",
            "SELECT status, count(*), sum(size), min(size), max(size), avg(size) FROM items GROUP BY status ORDER BY status",
            "SELECT category, status, count(*) FROM items GROUP BY category, status HAVING count(*) > 38 ORDER BY 1, 2",
            "SELECT count(DISTINCT status), count(size), count(*), total(size) FROM items",
            "SELECT DISTINCT category FROM items ORDER BY category DESC",
            "SELECT local_id FROM items WHERE size IS NULL ORDER BY local_id LIMIT 4",
            "SELECT local_id FROM items WHERE name LIKE 'file-1%.jpg' AND size BETWEEN 10000 AND 20000 ORDER BY 1",
            "SELECT local_id FROM items WHERE category IN ('photo', 'doc') AND status NOT IN ('done', 'failed') AND attempts >= 2 ORDER BY 1 LIMIT 6",
            "SELECT local_id, CASE WHEN size IS NULL THEN 'unknown' WHEN size < 1000 THEN 'small' ELSE 'big' END FROM items ORDER BY local_id LIMIT 12",
            "SELECT upper(category), length(name), substr(name, 6, 3), size / 3, size % 7, size * 1.5 FROM items WHERE local_id = 'id0010'",
            "SELECT coalesce(size, -1), ifnull(error, 'none'), typeof(size), typeof(error) FROM items WHERE local_id IN ('id0011', 'id0012', 'id0013') ORDER BY local_id",
            "SELECT category || '/' || local_id FROM items WHERE rowid = 5",
            "SELECT rowid, local_id FROM items WHERE rowid > 595 ORDER BY rowid",
            "SELECT count(*) FROM items WHERE rowid BETWEEN 100 AND 200",
            "SELECT local_id FROM items WHERE modified_ms > 1700000590000 ORDER BY modified_ms DESC",
            "SELECT status, count(*) FROM items WHERE category = 'photo' GROUP BY status ORDER BY count(*) DESC, status",
            "SELECT (SELECT count(*) FROM items WHERE status = 'done'), (SELECT max(size) FROM items)",
            "SELECT local_id FROM items WHERE size > (SELECT avg(size) FROM items) ORDER BY size DESC, local_id LIMIT 5",
            "SELECT local_id FROM items i WHERE EXISTS (SELECT 1 FROM items j WHERE j.category = i.category AND j.size > 99900) AND i.size < 100 ORDER BY 1",
            "SELECT x.status, x.n FROM (SELECT status, count(*) AS n FROM items GROUP BY status) AS x WHERE x.n > 100 ORDER BY x.status",
            "SELECT status FROM items WHERE category = 'doc' UNION SELECT status FROM items WHERE category = 'video' ORDER BY 1",
            "SELECT local_id FROM items WHERE status = 'done' EXCEPT SELECT local_id FROM items WHERE category = 'photo' ORDER BY 1 LIMIT 5",
            "SELECT group_concat(local_id) FROM (SELECT local_id FROM items ORDER BY local_id LIMIT 5)",
            "SELECT attempts, sum(size), avg(retry_at_ms) FROM items GROUP BY attempts ORDER BY attempts",
            "SELECT NOT (size > 100), size > 100 AND attempts = 1, size > 100 OR attempts = 1 FROM items WHERE local_id IN ('id0011', 'id0022', 'id0033') ORDER BY local_id",
            "SELECT 1 + 2 * 3, 7 / 2, 7 % 3, -7 / 2, 7.0 / 2, 1 = 1, 'a' < 'b', NULL IS NULL, 5 IS NOT NULL, 10 / 0, 2 << 3, 5 & 3, 5 | 3",
            "SELECT 9223372036854775807 + 1, 0.1 + 0.2, 1e20, 1.0, 100.0 / 3, 255 = 255.0",
            "SELECT 'abc' = 'ABC' COLLATE NOCASE, 'abc' < 'abd', '10' < '9', 10 < 9, '10' + 5, 'x' || 1 || 2.5",
            "SELECT abs(-5), abs(-5.5), round(2.567, 2), round(2.5), hex('ab'), quote('it''s'), replace('aXbXc', 'X', '-'), trim('  x  '), instr('hello', 'll')",
        ],
    ));
}

#[quench_test::test]
fn joins_over_two_tables_match_sqlite3() {
    let mut script = manifest();
    script.push(s(
        "CREATE TABLE cats (name TEXT PRIMARY KEY, weight INTEGER, label TEXT)",
    ));
    script.push(s("INSERT INTO cats VALUES ('photo', 3, 'Photos'), ('video', 5, 'Videos'), ('audio', 1, NULL)"));
    script.push(s(
        "CREATE TABLE notes (id INTEGER PRIMARY KEY, item TEXT, body TEXT)",
    ));
    for i in 1..=40 {
        script.push(format!(
            "INSERT INTO notes (item, body) VALUES ('id{:04}', 'note {i}')",
            i * 7
        ));
    }
    check(&with(
        script,
        &[
            "SELECT i.local_id, c.label FROM items i JOIN cats c ON c.name = i.category WHERE i.size > 99000 ORDER BY i.local_id",
            "SELECT i.local_id, c.label FROM items i LEFT JOIN cats c ON c.name = i.category WHERE i.local_id < 'id0010' ORDER BY i.local_id",
            "SELECT c.name, count(i.local_id) FROM cats c LEFT JOIN items i ON i.category = c.name AND i.status = 'done' GROUP BY c.name ORDER BY c.name",
            "SELECT n.id, i.name, n.body FROM notes n JOIN items i ON i.local_id = n.item AND i.category = 'doc' ORDER BY n.id",
            "SELECT n.id, i.category FROM notes n, items i WHERE i.local_id = n.item AND n.id < 6 ORDER BY n.id, i.category",
            "SELECT count(*) FROM notes n JOIN items i ON i.local_id = n.item",
            "SELECT cats.name, notes.id FROM cats CROSS JOIN notes WHERE notes.id <= 2 ORDER BY 1, 2",
            "SELECT * FROM cats c LEFT JOIN notes n ON n.id = c.weight ORDER BY c.name",
            "SELECT c.name FROM cats c JOIN cats d USING (name) ORDER BY 1",
            "SELECT name, (SELECT count(*) FROM items i WHERE i.category = c.name) FROM cats c ORDER BY 1",
        ],
    ));
}

#[quench_test::test]
fn writes_match_sqlite3() {
    check(&with(
        manifest(),
        &[
            "UPDATE items SET status = 'done', attempts = attempts + 1 WHERE category = 'photo' AND status = 'uploading'",
            "SELECT status, attempts, count(*) FROM items GROUP BY 1, 2 ORDER BY 1, 2",
            "UPDATE items SET error = 'x' || local_id, size = size * 2 WHERE rowid <= 20",
            "SELECT local_id, error, size FROM items WHERE rowid <= 22 ORDER BY rowid",
            "DELETE FROM items WHERE status = 'too_large' OR (size IS NULL AND category = 'doc')",
            "SELECT count(*), min(rowid), max(rowid) FROM items",
            "SELECT status, count(*) FROM items GROUP BY status ORDER BY status",
            "INSERT OR REPLACE INTO items (category, local_id, name, status) VALUES ('photo', 'id0003', 'replaced', 'pending')",
            "SELECT * FROM items WHERE category = 'photo' AND local_id = 'id0003'",
            "INSERT OR IGNORE INTO items (category, local_id, name) VALUES ('photo', 'id0003', 'ignored')",
            "SELECT name FROM items WHERE category = 'photo' AND local_id = 'id0003'",
            "INSERT INTO items (category, local_id, name, attempts) VALUES ('photo', 'id0003', 'upserted', 1) ON CONFLICT (category, local_id) DO UPDATE SET name = excluded.name, attempts = attempts + excluded.attempts",
            "SELECT name, attempts FROM items WHERE category = 'photo' AND local_id = 'id0003'",
            "INSERT INTO items (category, local_id) VALUES ('photo', 'id0003') ON CONFLICT DO NOTHING",
            "SELECT count(*) FROM items WHERE category = 'photo'",
            "DELETE FROM items",
            "SELECT count(*) FROM items",
            "INSERT INTO items (category, local_id) VALUES ('a', 'b')",
            "SELECT category, local_id, status, attempts, retry_at_ms, name FROM items",
        ],
    ));
}

#[quench_test::test]
fn rowid_tables_with_aliases_and_nulls_match_sqlite3() {
    check(&[
        s("CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b REAL, c, d NUMERIC, e INT)"),
        s(
            "INSERT INTO t (a, b, c, d, e) VALUES ('1', 2, '3', '4.0', 5.5), (NULL, NULL, NULL, NULL, NULL), ('x', 'y', 'z', 'w', 'v')",
        ),
        s("INSERT INTO t (id, a) VALUES (100, 'hundred'), (50, 'fifty')"),
        s("INSERT INTO t (a) VALUES ('after')"),
        s(
            "SELECT id, a, typeof(a), b, typeof(b), c, typeof(c), d, typeof(d), e, typeof(e) FROM t ORDER BY id",
        ),
        s("SELECT id FROM t WHERE a = 1 ORDER BY id"),
        s("SELECT id FROM t WHERE d = 4 ORDER BY id"),
        s("SELECT id FROM t WHERE e > 5 ORDER BY id"),
        s("SELECT id FROM t WHERE c = 3 ORDER BY id"),
        s("SELECT id FROM t WHERE c = '3' ORDER BY id"),
        s("SELECT id, a FROM t ORDER BY a"),
        s("SELECT id, a FROM t ORDER BY a DESC"),
        s("SELECT id, a FROM t ORDER BY a NULLS LAST"),
        s("SELECT count(*), count(a), count(b) FROM t"),
        s("UPDATE t SET id = id + 1000 WHERE id < 10"),
        s("SELECT id FROM t ORDER BY id"),
        s("SELECT max(id) + 1 FROM t"),
        s("DELETE FROM t WHERE id > 1000"),
        s("SELECT id FROM t ORDER BY id"),
    ]);
}

/// Opens the file again, waiting out a lock a forked `sqlite3` of a parallel test is still holding.
fn reopen(path: &std::path::Path) -> Connection {
    for _ in 0..200 {
        match Connection::open(path) {
            Err(Error::Busy) => std::thread::sleep(std::time::Duration::from_millis(5)),
            other => return other.expect("reopen"),
        }
    }
    Connection::open(path).expect("reopen")
}

fn run(conn: &mut Connection, sql: &str) -> Result<crate::Rows, Error> {
    conn.query(sql, &[])
}

#[quench_test::test]
fn constraints_and_conflicts_are_errors_with_the_sqlite_wording() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let mut c = Connection::create(dir.path().join("c.db")).expect("create");
    run(&mut c, "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, n INTEGER CHECK (n >= 0), k TEXT)").expect("create");
    run(&mut c, "INSERT INTO t (id, name, n) VALUES (1, 'a', 1)").expect("insert");
    for (sql, expect) in [
        (
            "INSERT INTO t (id, name) VALUES (1, 'b')",
            "UNIQUE constraint failed: t.id",
        ),
        (
            "INSERT INTO t (id, name) VALUES (2, 'a')",
            "UNIQUE constraint failed: t.name",
        ),
        (
            "INSERT INTO t (id) VALUES (3)",
            "NOT NULL constraint failed: t.name",
        ),
        (
            "INSERT INTO t (id, name, n) VALUES (4, 'd', -1)",
            "CHECK constraint failed",
        ),
        (
            "UPDATE t SET name = NULL",
            "NOT NULL constraint failed: t.name",
        ),
    ] {
        let e = run(&mut c, sql).expect_err(sql);
        assert!(matches!(e, Error::Constraint(_)), "{sql}: {e}");
        assert!(e.to_string().contains(expect), "{sql}: {e}");
    }
    // A failed statement changed nothing, even half-way through a multi-row insert.
    let e = run(
        &mut c,
        "INSERT INTO t (id, name) VALUES (10, 'x'), (11, 'y'), (1, 'z')",
    );
    assert!(e.is_err());
    assert_eq!(
        run(&mut c, "SELECT count(*) FROM t").expect("count").rows,
        vec![vec![Value::Integer(1)]]
    );
    assert!(run(&mut c, "SELECT nope FROM t").is_err());
    assert!(run(&mut c, "SELECT * FROM nope").is_err());
    assert!(run(&mut c, "INSERT INTO t (id, name) VALUES (1)").is_err());
}

#[quench_test::test]
fn transactions_savepoints_and_a_failed_statement_inside_one() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("x.db");
    let mut c = Connection::create(&path).expect("create");
    c.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT UNIQUE)",
        &[],
    )
    .expect("create");
    c.execute("BEGIN", &[]).expect("begin");
    c.execute("INSERT INTO t (v) VALUES ('a')", &[]).expect("a");
    assert!(
        c.execute("INSERT INTO t (v) VALUES ('b'), ('a')", &[])
            .is_err(),
        "the whole statement fails"
    );
    c.execute("SAVEPOINT s1", &[]).expect("savepoint");
    c.execute("INSERT INTO t (v) VALUES ('c')", &[]).expect("c");
    c.execute("CREATE TABLE u (x)", &[])
        .expect("ddl in a savepoint");
    c.execute("ROLLBACK TO s1", &[]).expect("rollback to");
    assert!(
        c.query("SELECT * FROM u", &[]).is_err(),
        "the table made after the savepoint is gone"
    );
    c.execute("INSERT INTO t (v) VALUES ('d')", &[]).expect("d");
    c.execute("RELEASE s1", &[]).expect("release");
    c.execute("COMMIT", &[]).expect("commit");
    drop(c);
    let mut c = reopen(&path);
    let rows = c
        .query("SELECT v FROM t ORDER BY id", &[])
        .expect("select")
        .rows;
    assert_eq!(
        rows,
        vec![vec![Value::Text("a".into())], vec![Value::Text("d".into())]]
    );
    c.execute("BEGIN", &[]).expect("begin");
    c.execute("DELETE FROM t", &[]).expect("delete");
    c.execute("ROLLBACK", &[]).expect("rollback");
    assert_eq!(
        c.query("SELECT count(*) FROM t", &[]).expect("count").rows[0][0],
        Value::Integer(2)
    );
    let r: Result<(), Error> = c.transaction(|c| {
        c.execute("DELETE FROM t", &[])?;
        Err(Error::Unsupported("stop".into()))
    });
    assert!(r.is_err());
    assert_eq!(
        c.query("SELECT count(*) FROM t", &[]).expect("count").rows[0][0],
        Value::Integer(2)
    );
}

#[quench_test::test]
fn parameters_returning_alter_and_pragmas() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let mut c = Connection::create(dir.path().join("p.db")).expect("create");
    c.execute(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT, b INTEGER DEFAULT 7)",
        &[],
    )
    .expect("create");
    c.execute(
        "INSERT INTO t (a, b) VALUES (?, ?), (?2, ?1)",
        &[Value::Integer(1), Value::Text("x".into())],
    )
    .expect("insert");
    let r = c
        .query("SELECT a, b FROM t ORDER BY id", &[])
        .expect("select");
    assert_eq!(
        r.rows[0],
        vec![Value::Text("1".into()), Value::Text("x".into())],
        "'x' cannot be an integer: it stays text"
    );
    let r = c
        .query_named(
            "INSERT INTO t (a) VALUES (:name) RETURNING id, a, b",
            &[],
            &[(":name", Value::Text("n".into()))],
        )
        .expect("returning");
    assert_eq!(r.columns, ["id", "a", "b"]);
    assert_eq!(
        r.rows,
        vec![vec![
            Value::Integer(3),
            Value::Text("n".into()),
            Value::Integer(7)
        ]]
    );
    assert_eq!(c.last_insert_rowid(), 3);
    assert_eq!(c.changes(), 1);
    let r = c
        .query(
            "UPDATE t SET b = b + 1 WHERE id >= ? RETURNING id, b",
            &[Value::Integer(2)],
        )
        .expect("update returning");
    assert_eq!(r.rows.len(), 2);
    assert_eq!(
        c.execute("DELETE FROM t WHERE id = ?", &[Value::Integer(3)])
            .expect("delete"),
        1
    );
    // ALTER TABLE ADD COLUMN: old rows read the default, new rows can set it.
    c.execute("ALTER TABLE t ADD COLUMN extra TEXT DEFAULT 'dflt'", &[])
        .expect("alter");
    c.execute("INSERT INTO t (a, extra) VALUES ('new', 'given')", &[])
        .expect("insert");
    let r = c
        .query("SELECT id, extra FROM t ORDER BY id", &[])
        .expect("select");
    assert_eq!(r.rows[0][1], Value::Text("dflt".into()));
    assert_eq!(r.rows[2][1], Value::Text("given".into()));
    assert!(
        c.execute("ALTER TABLE t ADD COLUMN bad TEXT NOT NULL", &[])
            .is_err()
    );
    // Pragmas.
    assert_eq!(
        c.query("PRAGMA user_version", &[]).expect("pragma").rows[0][0],
        Value::Integer(0)
    );
    c.execute("PRAGMA user_version = 42", &[]).expect("set");
    assert_eq!(
        c.query("PRAGMA user_version", &[]).expect("pragma").rows[0][0],
        Value::Integer(42)
    );
    let info = c.query("PRAGMA table_info(t)", &[]).expect("table_info");
    assert_eq!(info.rows.len(), 4);
    assert_eq!(info.rows[0][1], Value::Text("id".into()));
    assert_eq!(info.rows[0][5], Value::Integer(1));
    assert!(c.query("PRAGMA nonsense", &[]).is_err());
}

#[quench_test::test]
fn the_database_written_here_passes_sqlite3_integrity_and_returns_the_same_answers() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("w.db");
    let mut c = Connection::create(&path).expect("create");
    for st in manifest() {
        c.execute(&st, &[]).expect(&st);
    }
    c.execute("DELETE FROM items WHERE status = 'done'", &[])
        .expect("delete");
    c.execute(
        "UPDATE items SET status = 'failed' WHERE local_id < 'id0100'",
        &[],
    )
    .expect("update");
    let ours = c
        .query(
            "SELECT status, count(*), sum(size) FROM items GROUP BY status ORDER BY status",
            &[],
        )
        .expect("q");
    drop(c);
    let check = |sql: &str| {
        Command::new("sqlite3")
            .arg(&path)
            .arg(sql)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    if let Some(r) = check("PRAGMA integrity_check") {
        assert_eq!(r, "ok");
        let theirs =
            check("SELECT status, count(*), sum(size) FROM items GROUP BY status ORDER BY status")
                .expect("sqlite3");
        let mine: Vec<String> = ours
            .rows
            .iter()
            .map(|r| r.iter().map(format_value).collect::<Vec<_>>().join("|"))
            .collect();
        assert_eq!(theirs, mine.join("\n"));
        assert_eq!(
            check("SELECT count(*) FROM items WHERE category = 'photo' AND local_id = 'id0003'")
                .expect("q"),
            "1"
        );
    }
}

/// Pages the file gave up while `sql` ran, with a cold cache.
fn pages_read(c: &mut Connection, sql: &str) -> u64 {
    c.database().drop_cache();
    let before = c.database().file_reads();
    c.query(sql, &[]).expect(sql);
    c.database().file_reads() - before
}

#[quench_test::test]
fn lookups_use_the_keys_and_indexes_instead_of_reading_the_table() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let mut c = Connection::create(dir.path().join("big.db")).expect("create");
    c.execute(
        "CREATE TABLE big (id INTEGER PRIMARY KEY, k TEXT NOT NULL UNIQUE, grp INTEGER, v TEXT)",
        &[],
    )
    .expect("create");
    c.execute("CREATE INDEX big_grp ON big (grp)", &[])
        .expect("index");
    c.transaction(|c| {
        for i in 1..=12_000_i64 {
            c.execute(
                "INSERT INTO big (k, grp, v) VALUES (?, ?, ?)",
                &[
                    Value::Text(format!("key{i:06}")),
                    Value::Integer(i % 1000),
                    Value::Text("x".repeat(200)),
                ],
            )?;
        }
        Ok(())
    })
    .expect("fill");
    let total = c.database().page_count();
    assert!(total > 500, "a table of {total} pages");
    let by_rowid = pages_read(&mut c, "SELECT v FROM big WHERE id = 6345");
    let by_unique = pages_read(&mut c, "SELECT v FROM big WHERE k = 'key006345'");
    let by_index = pages_read(&mut c, "SELECT count(*) FROM big WHERE grp = 7");
    let by_range = pages_read(
        &mut c,
        "SELECT count(*) FROM big WHERE id BETWEEN 100 AND 200",
    );
    let limited = pages_read(&mut c, "SELECT id FROM big LIMIT 5");
    let in_join = pages_read(
        &mut c,
        "SELECT b.v FROM big a JOIN big b ON b.id = a.id + 1 WHERE a.id = 77",
    );
    let full = pages_read(&mut c, "SELECT count(*) FROM big WHERE v = 'nothing'");
    assert!(by_rowid <= 6, "rowid lookup read {by_rowid} pages");
    assert!(by_unique <= 12, "unique lookup read {by_unique} pages");
    assert!(
        by_index < u64::from(total) / 5,
        "an index lookup read {by_index} of {total} pages"
    );
    assert!(by_range <= 12, "a rowid range read {by_range} pages");
    assert!(limited <= 6, "LIMIT 5 read {limited} pages");
    assert!(in_join <= 14, "a join on the rowid read {in_join} pages");
    assert!(
        full > u64::from(total) / 2,
        "a full scan reads the table: {full} of {total}"
    );
    // And the answers are right.
    let r = c
        .query("SELECT count(*) FROM big WHERE grp = 7", &[])
        .expect("q");
    assert_eq!(r.rows[0][0], Value::Integer(12));
    let r = c
        .query(
            "SELECT k FROM big WHERE grp = 999 AND id > 11000 ORDER BY id LIMIT 2",
            &[],
        )
        .expect("q");
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0][0], Value::Text("key011999".into()));
}

/// The examples of `docs/crates/quench-sqlite/usage.md`, run as written (with a temporary path).
#[quench_test::test]
fn the_examples_of_the_usage_page_run() {
    let dir = quench_core::tmp::TempDir::new().expect("tmp");
    let path = dir.path().join("app.db");
    let mut db = Connection::open_or_create(&path).expect("open_or_create");
    assert!(
        Connection::create(&path).is_err(),
        "create fails if the file is there"
    );
    db.execute(
        "CREATE TABLE IF NOT EXISTS items (
            category TEXT NOT NULL, local_id TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'pending',
            attempts INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (category, local_id))",
        &[],
    )
    .expect("create");
    db.execute(
        "CREATE INDEX IF NOT EXISTS items_status ON items (status, attempts)",
        &[],
    )
    .expect("index");
    let changed = db
        .execute(
            "INSERT INTO items (category, local_id) VALUES (?, ?)",
            &[Value::Text("photo".into()), Value::Text("id1".into())],
        )
        .expect("insert");
    assert_eq!(changed, 1);
    assert_eq!(db.last_insert_rowid(), 1);
    // 3. parameters
    let rows = db
        .query(
            "SELECT local_id FROM items WHERE category = ?1 AND attempts <= ?2 ORDER BY local_id LIMIT 50",
            &[Value::Text("photo".into()), Value::Integer(3)],
        )
        .expect("query");
    assert_eq!(rows.rows.len(), 1);
    let rows = db
        .query_named(
            "SELECT * FROM items WHERE status = :status",
            &[],
            &[(":status", Value::Text("done".into()))],
        )
        .expect("named");
    assert!(rows.rows.is_empty());
    // 4. transactions
    db.transaction(|db| {
        for i in 0..1000 {
            db.execute(
                "INSERT INTO items (category, local_id) VALUES ('doc', ?)",
                &[Value::Text(format!("id{i}"))],
            )?;
        }
        Ok(())
    })
    .expect("transaction");
    // 5. upsert and RETURNING
    db.execute(
        "INSERT INTO items (category, local_id, attempts) VALUES ('photo', 'id1', 1)
         ON CONFLICT (category, local_id) DO UPDATE SET attempts = attempts + excluded.attempts",
        &[],
    )
    .expect("upsert");
    let r = db
        .query(
            "UPDATE items SET status = 'done' WHERE local_id = 'id1' RETURNING category, status",
            &[],
        )
        .expect("returning");
    assert_eq!(r.columns, ["category", "status"]);
    // 6. a schema that evolves
    let version = db.query("PRAGMA user_version", &[]).expect("pragma").rows[0][0]
        .as_integer()
        .unwrap_or(0);
    assert_eq!(version, 0);
    db.transaction(|db| {
        db.execute("ALTER TABLE items ADD COLUMN error TEXT", &[])?;
        db.execute("PRAGMA user_version = 1", &[])?;
        Ok(())
    })
    .expect("evolve");
    assert_eq!(
        db.query("PRAGMA user_version", &[]).expect("pragma").rows[0][0],
        Value::Integer(1)
    );
    drop(db);
    // 7. check a file
    let mut check = crate::Database::open(&path).expect("open");
    assert!(check.integrity_check().expect("check").is_empty());
    drop(check);
    // 8. the storage layer
    let raw = dir.path().join("raw.db");
    let mut low = crate::Database::create(&raw, 4096).expect("create");
    let root = low
        .write(|db| db.create_table("kv", "CREATE TABLE kv (k INTEGER PRIMARY KEY, v)"))
        .expect("table");
    low.write(|db| db.insert_row(root, 1, &[Value::Null, Value::Text("one".into())]))
        .expect("insert");
    assert_eq!(
        low.get_row(root, 1).expect("get"),
        Some(vec![Value::Null, Value::Text("one".into())])
    );
}
