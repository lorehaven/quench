//! The SQL parser: shapes, precedence, errors, and a corpus the stock `sqlite3` agrees with.

use std::process::Command;

use crate::sql::ast::{
    BinaryOp, ColumnConstraint, Expr, InsertSource, JoinKind, LikeKind, Param, ResultColumn, Stmt,
    TableConstraint, TableSource, UnaryOp, UpsertAction,
};
use crate::sql::{parse, parse_one};
use crate::{Error, Value};

fn expr(sql: &str) -> Expr {
    let Stmt::Select(select) =
        parse_one(&format!("SELECT {sql}")).unwrap_or_else(|e| panic!("{sql}: {e}"))
    else {
        panic!("not a select");
    };
    let ResultColumn::Expr { expr, .. } = select.core.columns.into_iter().next().expect("a column")
    else {
        panic!("not an expression");
    };
    expr
}

fn int(n: i64) -> Expr {
    Expr::Literal(Value::Integer(n))
}

fn col(name: &str) -> Expr {
    Expr::Column {
        table: None,
        name: name.into(),
    }
}

fn bin(op: BinaryOp, l: Expr, r: Expr) -> Expr {
    Expr::Binary(op, Box::new(l), Box::new(r))
}

#[quench_test::test]
fn arithmetic_binds_tighter_than_comparison_than_and_than_or() {
    assert_eq!(
        expr("1 + 2 * 3"),
        bin(BinaryOp::Add, int(1), bin(BinaryOp::Mul, int(2), int(3)))
    );
    assert_eq!(
        expr("1 - 2 - 3"),
        bin(BinaryOp::Sub, bin(BinaryOp::Sub, int(1), int(2)), int(3)),
        "left associative"
    );
    assert_eq!(
        expr("a OR b AND c"),
        bin(
            BinaryOp::Or,
            col("a"),
            bin(BinaryOp::And, col("b"), col("c"))
        )
    );
    assert_eq!(
        expr("a = 1 AND b < 2 + 3"),
        bin(
            BinaryOp::And,
            bin(BinaryOp::Eq, col("a"), int(1)),
            bin(BinaryOp::Lt, col("b"), bin(BinaryOp::Add, int(2), int(3)))
        )
    );
    assert_eq!(
        expr("a || b + c"),
        bin(
            BinaryOp::Add,
            bin(BinaryOp::Concat, col("a"), col("b")),
            col("c")
        ),
        "|| binds tighter than +"
    );
    assert_eq!(
        expr("a & 1 = 2"),
        bin(
            BinaryOp::Eq,
            bin(BinaryOp::BitAnd, col("a"), int(1)),
            int(2)
        )
    );
}

#[quench_test::test]
fn not_is_looser_than_comparison_and_unary_minus_is_tightest() {
    assert_eq!(
        expr("NOT a = b"),
        Expr::Unary(
            UnaryOp::Not,
            Box::new(bin(BinaryOp::Eq, col("a"), col("b")))
        )
    );
    assert_eq!(expr("-5"), int(-5));
    assert_eq!(
        expr("-a * 2"),
        bin(
            BinaryOp::Mul,
            Expr::Unary(UnaryOp::Neg, Box::new(col("a"))),
            int(2)
        )
    );
    assert_eq!(expr("- -3"), int(3));
    assert_eq!(expr("1.5"), Expr::Literal(Value::Real(1.5)));
    assert_eq!(expr("-1.5e2"), Expr::Literal(Value::Real(-150.0)));
    assert_eq!(expr("9223372036854775807"), int(i64::MAX));
    assert_eq!(
        expr("9223372036854775808"),
        Expr::Literal(Value::Real(9_223_372_036_854_775_808.0))
    );
    assert_eq!(expr("0xff"), int(255));
    assert_eq!(
        expr("x'cafe'"),
        Expr::Literal(Value::Blob(vec![0xca, 0xfe]))
    );
    assert_eq!(expr("'it''s'"), Expr::Literal(Value::Text("it's".into())));
    assert_eq!(expr("TRUE"), int(1));
    assert_eq!(expr("null"), Expr::Literal(Value::Null));
}

#[quench_test::test]
fn is_in_between_like_and_their_negations() {
    assert!(matches!(
        expr("a IS NULL"),
        Expr::IsNull { negated: false, .. }
    ));
    assert!(matches!(
        expr("a IS NOT NULL"),
        Expr::IsNull { negated: true, .. }
    ));
    assert!(matches!(
        expr("a NOT NULL"),
        Expr::IsNull { negated: true, .. }
    ));
    assert!(matches!(
        expr("a ISNULL"),
        Expr::IsNull { negated: false, .. }
    ));
    assert_eq!(expr("a IS b"), bin(BinaryOp::Is, col("a"), col("b")));
    assert_eq!(expr("a IS NOT b"), bin(BinaryOp::IsNot, col("a"), col("b")));
    assert_eq!(
        expr("a IS DISTINCT FROM b"),
        bin(BinaryOp::IsNot, col("a"), col("b"))
    );
    assert_eq!(
        expr("a IS NOT DISTINCT FROM b"),
        bin(BinaryOp::Is, col("a"), col("b"))
    );
    let Expr::InList { list, negated, .. } = expr("a NOT IN (1, 2, 3)") else {
        panic!("in list");
    };
    assert_eq!((list.len(), negated), (3, true));
    assert!(matches!(expr("a IN ()"), Expr::InList { ref list, .. } if list.is_empty()));
    assert!(matches!(
        expr("a IN (SELECT b FROM t)"),
        Expr::InSelect { negated: false, .. }
    ));
    // BETWEEN's AND is its own.
    assert_eq!(
        expr("a BETWEEN 1 AND 2 AND c"),
        bin(
            BinaryOp::And,
            Expr::Between {
                expr: Box::new(col("a")),
                low: Box::new(int(1)),
                high: Box::new(int(2)),
                negated: false
            },
            col("c")
        )
    );
    let Expr::Like {
        kind,
        escape,
        negated,
        ..
    } = expr("a NOT LIKE 'x%' ESCAPE '\\'")
    else {
        panic!("like");
    };
    assert_eq!(
        (kind, escape.is_some(), negated),
        (LikeKind::Like, true, true)
    );
    assert!(matches!(
        expr("a GLOB 'x*'"),
        Expr::Like {
            kind: LikeKind::Glob,
            ..
        }
    ));
    assert!(matches!(
        expr("a COLLATE nocase = 'x'"),
        Expr::Binary(BinaryOp::Eq, ..)
    ));
}

#[quench_test::test]
fn functions_case_cast_exists_and_subqueries() {
    assert_eq!(
        expr("count(*)"),
        Expr::Function {
            name: "count".into(),
            args: vec![],
            distinct: false,
            star: true
        }
    );
    let Expr::Function {
        name,
        args,
        distinct,
        ..
    } = expr("COUNT(DISTINCT a)")
    else {
        panic!("function");
    };
    assert_eq!((name.as_str(), args.len(), distinct), ("count", 1, true));
    assert!(matches!(expr("coalesce(a, 0)"), Expr::Function { ref args, .. } if args.len() == 2));
    assert!(matches!(expr("now()"), Expr::Function { ref args, .. } if args.is_empty()));
    let Expr::Case {
        operand,
        whens,
        otherwise,
    } = expr("CASE a WHEN 1 THEN 'x' WHEN 2 THEN 'y' ELSE 'z' END")
    else {
        panic!("case");
    };
    assert_eq!(
        (operand.is_some(), whens.len(), otherwise.is_some()),
        (true, 2, true)
    );
    assert!(matches!(
        expr("CASE WHEN a THEN 1 END"),
        Expr::Case {
            operand: None,
            otherwise: None,
            ..
        }
    ));
    assert!(
        matches!(expr("CAST(a AS INTEGER)"), Expr::Cast { ref type_name, .. } if type_name == "INTEGER")
    );
    assert!(
        matches!(expr("CAST(a AS VARCHAR(10))"), Expr::Cast { ref type_name, .. } if type_name == "VARCHAR(10)")
    );
    assert!(matches!(expr("EXISTS (SELECT 1)"), Expr::Exists(_)));
    assert!(matches!(
        expr("NOT EXISTS (SELECT 1)"),
        Expr::Unary(UnaryOp::Not, _)
    ));
    assert!(matches!(expr("(SELECT max(a) FROM t)"), Expr::Subquery(_)));
    assert_eq!(
        expr("(1 + 2) * 3"),
        bin(BinaryOp::Mul, bin(BinaryOp::Add, int(1), int(2)), int(3))
    );
    assert_eq!(
        expr("t.a"),
        Expr::Column {
            table: Some("t".into()),
            name: "a".into()
        }
    );
    assert_eq!(expr("\"my col\""), col("my col"));
    assert_eq!(expr("[my col]"), col("my col"));
    assert_eq!(expr("`x`"), col("x"));
}

#[quench_test::test]
fn parameters_are_numbered_the_way_sqlite_does() {
    let Stmt::Select(s) =
        parse_one("SELECT ?, ?, ?5, ?, :name, @n, $m, :name FROM t").expect("parse")
    else {
        panic!("select");
    };
    let params: Vec<Param> = s
        .core
        .columns
        .iter()
        .map(|c| match c {
            ResultColumn::Expr {
                expr: Expr::Param(p),
                ..
            } => p.clone(),
            _ => panic!("param"),
        })
        .collect();
    assert_eq!(
        params,
        [
            Param::Index(1),
            Param::Index(2),
            Param::Index(5),
            Param::Index(6),
            Param::Named("name".into()),
            Param::Named("n".into()),
            Param::Named("m".into()),
            Param::Named("name".into()),
        ]
    );
    assert!(parse_one("SELECT ?0").is_err());
}

#[quench_test::test]
fn select_with_joins_grouping_ordering_and_limits() {
    let Stmt::Select(s) = parse_one(
        "SELECT DISTINCT a.x AS ex, count(*) n, b.* FROM a LEFT OUTER JOIN b ON a.id = b.id, c \
         INNER JOIN (SELECT 1 AS k) sub USING (k) WHERE a.x > 1 GROUP BY a.x HAVING count(*) > 2 \
         ORDER BY n DESC NULLS LAST, 1 LIMIT 10 OFFSET 5",
    )
    .expect("parse") else {
        panic!("select");
    };
    assert!(s.core.distinct);
    assert_eq!(s.core.columns.len(), 3);
    assert!(matches!(&s.core.columns[0], ResultColumn::Expr { alias: Some(a), .. } if a == "ex"));
    assert!(matches!(&s.core.columns[1], ResultColumn::Expr { alias: Some(a), .. } if a == "n"));
    assert!(matches!(&s.core.columns[2], ResultColumn::TableAll(t) if t == "b"));
    let from = s.core.from.expect("from");
    assert!(matches!(&from.first, TableSource::Table { name, alias: None } if name == "a"));
    let kinds: Vec<JoinKind> = from.joins.iter().map(|j| j.kind).collect();
    assert_eq!(kinds, [JoinKind::Left, JoinKind::Cross, JoinKind::Inner]);
    assert!(
        matches!(&from.joins[2].source, TableSource::Subquery { alias: Some(a), .. } if a == "sub")
    );
    assert_eq!(s.core.group_by.len(), 1);
    assert!(s.core.having.is_some());
    assert_eq!(s.order_by.len(), 2);
    assert!(s.order_by[0].descending);
    assert_eq!((s.limit, s.offset), (Some(int(10)), Some(int(5))));
    let Stmt::Select(s) = parse_one("SELECT 1 LIMIT 2, 3").expect("parse") else {
        panic!("select");
    };
    assert_eq!(
        (s.limit, s.offset),
        (Some(int(3)), Some(int(2))),
        "LIMIT offset, count"
    );
    let Stmt::Select(s) =
        parse_one("SELECT 1 UNION ALL SELECT 2 UNION SELECT 3 ORDER BY 1").expect("parse")
    else {
        panic!("select");
    };
    assert_eq!(s.compounds.len(), 2);
}

#[quench_test::test]
fn insert_forms_and_upsert() {
    let Stmt::Insert(i) =
        parse_one("INSERT INTO t (a, b) VALUES (1, 'x'), (?, ?) RETURNING a, b").expect("parse")
    else {
        panic!("insert");
    };
    assert_eq!(
        i.columns.as_deref(),
        Some(&["a".to_string(), "b".to_string()][..])
    );
    assert!(matches!(&i.source, InsertSource::Values(rows) if rows.len() == 2));
    assert_eq!(i.returning.len(), 2);
    let Stmt::Insert(i) = parse_one("REPLACE INTO t VALUES (1)").expect("parse") else {
        panic!("insert");
    };
    assert_eq!(i.or, crate::sql::ast::OnConflict::Replace);
    assert!(matches!(
        parse_one("INSERT OR IGNORE INTO t SELECT * FROM u"),
        Ok(Stmt::Insert(_))
    ));
    assert!(matches!(
        parse_one("INSERT INTO t DEFAULT VALUES"),
        Ok(Stmt::Insert(i)) if i.source == InsertSource::DefaultValues
    ));
    let Stmt::Insert(i) = parse_one(
        "INSERT INTO t (k, v) VALUES (1, 2) ON CONFLICT (k) DO UPDATE SET v = excluded.v + 1, n = n + 1 WHERE v < 9",
    )
    .expect("parse") else {
        panic!("insert");
    };
    let up = i.upsert.expect("upsert");
    assert_eq!(up.target, ["k"]);
    assert!(
        matches!(up.action, UpsertAction::Update { ref sets, where_clause: Some(_) } if sets.len() == 2)
    );
    let Stmt::Insert(i) =
        parse_one("INSERT INTO t VALUES (1) ON CONFLICT DO NOTHING").expect("parse")
    else {
        panic!("insert");
    };
    assert!(
        matches!(i.upsert, Some(u) if u.action == UpsertAction::Nothing && u.target.is_empty())
    );
}

#[quench_test::test]
fn update_delete_and_transactions() {
    let Stmt::Update(u) =
        parse_one("UPDATE t SET a = a + 1, b = NULL WHERE c IN (1, 2) RETURNING *").expect("parse")
    else {
        panic!("update");
    };
    assert_eq!(
        (u.sets.len(), u.where_clause.is_some(), u.returning.len()),
        (2, true, 1)
    );
    let Stmt::Delete(d) =
        parse_one("DELETE FROM main.t WHERE a NOT IN (SELECT a FROM u)").expect("parse")
    else {
        panic!("delete");
    };
    assert_eq!(d.table, "t");
    assert_eq!(
        parse_one("BEGIN").expect("parse"),
        Stmt::Begin(crate::sql::ast::TransactionMode::Deferred)
    );
    assert_eq!(
        parse_one("BEGIN IMMEDIATE TRANSACTION").expect("parse"),
        Stmt::Begin(crate::sql::ast::TransactionMode::Immediate)
    );
    assert_eq!(parse_one("COMMIT").expect("parse"), Stmt::Commit);
    assert_eq!(parse_one("END").expect("parse"), Stmt::Commit);
    assert_eq!(parse_one("ROLLBACK").expect("parse"), Stmt::Rollback);
    assert_eq!(
        parse_one("ROLLBACK TO SAVEPOINT s").expect("parse"),
        Stmt::RollbackTo("s".into())
    );
    assert_eq!(
        parse_one("SAVEPOINT s").expect("parse"),
        Stmt::Savepoint("s".into())
    );
    assert_eq!(
        parse_one("RELEASE s").expect("parse"),
        Stmt::Release("s".into())
    );
}

#[quench_test::test]
fn create_table_with_every_kind_of_constraint() {
    let Stmt::CreateTable(t) = parse_one(
        "CREATE TABLE IF NOT EXISTS items (\
           id INTEGER PRIMARY KEY AUTOINCREMENT, \
           category TEXT NOT NULL DEFAULT 'photo', \
           local_id VARCHAR(255) NOT NULL COLLATE NOCASE, \
           size UNSIGNED BIG INT DEFAULT -1 CHECK (size >= -1), \
           ratio DOUBLE PRECISION, \
           data, \
           UNIQUE (category, local_id DESC), \
           CONSTRAINT pk PRIMARY KEY (category, local_id), \
           CHECK (length(category) > 0)) WITHOUT ROWID",
    )
    .expect("parse") else {
        panic!("create table");
    };
    assert!(t.if_not_exists && t.without_rowid);
    assert_eq!(t.columns.len(), 6);
    assert_eq!(t.columns[0].type_name.as_deref(), Some("INTEGER"));
    assert!(matches!(
        t.columns[0].constraints[0],
        ColumnConstraint::PrimaryKey {
            autoincrement: true,
            ..
        }
    ));
    assert_eq!(t.columns[1].constraints.len(), 2);
    assert_eq!(t.columns[2].type_name.as_deref(), Some("VARCHAR(255)"));
    assert_eq!(t.columns[3].type_name.as_deref(), Some("UNSIGNED BIG INT"));
    assert!(matches!(&t.columns[3].constraints[0], ColumnConstraint::Default(e) if *e == int(-1)));
    assert_eq!(t.columns[4].type_name.as_deref(), Some("DOUBLE PRECISION"));
    assert_eq!(t.columns[5].type_name, None);
    assert_eq!(t.constraints.len(), 3);
    assert!(
        matches!(&t.constraints[0], TableConstraint::Unique(c) if c.len() == 2 && c[1].descending)
    );
    assert!(matches!(&t.constraints[1], TableConstraint::PrimaryKey(c) if c.len() == 2));
}

#[quench_test::test]
fn indexes_drops_alters_and_pragmas() {
    let Stmt::CreateIndex(i) =
        parse_one("CREATE UNIQUE INDEX IF NOT EXISTS i ON t (a, b DESC)").expect("parse")
    else {
        panic!("index");
    };
    assert!(i.unique && i.if_not_exists);
    assert_eq!(i.columns.len(), 2);
    assert_eq!(
        parse_one("DROP TABLE IF EXISTS t").expect("parse"),
        Stmt::DropTable {
            name: "t".into(),
            if_exists: true
        }
    );
    assert!(matches!(
        parse_one("DROP INDEX i"),
        Ok(Stmt::DropIndex {
            if_exists: false,
            ..
        })
    ));
    assert!(matches!(
        parse_one("ALTER TABLE t ADD COLUMN c TEXT DEFAULT 'x'"),
        Ok(Stmt::AlterTable { .. })
    ));
    assert!(matches!(
        parse_one("ALTER TABLE t RENAME TO u"),
        Ok(Stmt::AlterTable { .. })
    ));
    assert!(matches!(
        parse_one("ALTER TABLE t RENAME COLUMN a TO b"),
        Ok(Stmt::AlterTable { .. })
    ));
    assert!(matches!(
        parse_one("ALTER TABLE t DROP COLUMN a"),
        Ok(Stmt::AlterTable { .. })
    ));
    assert_eq!(
        parse_one("PRAGMA user_version = 3").expect("parse"),
        Stmt::Pragma {
            name: "user_version".into(),
            value: Some(Value::Integer(3))
        }
    );
    assert_eq!(
        parse_one("PRAGMA journal_mode(wal)").expect("parse"),
        Stmt::Pragma {
            name: "journal_mode".into(),
            value: Some(Value::Text("wal".into()))
        }
    );
    assert_eq!(
        parse_one("PRAGMA integrity_check").expect("parse"),
        Stmt::Pragma {
            name: "integrity_check".into(),
            value: None
        }
    );
}

#[quench_test::test]
fn several_statements_comments_and_whitespace() {
    let stmts = parse("-- first\nSELECT 1; /* second */ ;; SELECT 2 ;\n").expect("parse");
    assert_eq!(stmts.len(), 2);
    assert!(parse("").expect("parse").is_empty());
    assert!(parse_one("").is_err());
    assert!(
        parse_one("SELECT 1; SELECT 2").is_err(),
        "parse_one wants exactly one"
    );
    assert!(
        matches!(
            parse_one("select 1 from T where A = 1"),
            Ok(Stmt::Select(_))
        ),
        "keywords are case blind"
    );
}

#[quench_test::test]
fn errors_say_where_and_what_is_not_supported_says_so() {
    let Err(Error::Syntax { pos, message }) = parse_one("SELECT a FROM t WHERE") else {
        panic!("error expected");
    };
    assert_eq!(pos, 21);
    assert!(message.contains("expression"), "{message}");
    let Err(Error::Syntax { pos, .. }) = parse_one("SELECT 1 +") else {
        panic!("error expected");
    };
    assert_eq!(pos, 10);
    let Err(Error::Syntax { pos, message }) = parse_one("SELECT 'abc") else {
        panic!("error expected");
    };
    assert_eq!((pos, message.as_str()), (7, "unterminated quoted text"));
    for sql in [
        "WITH x AS (SELECT 1) SELECT * FROM x",
        "SELECT * FROM a NATURAL JOIN b",
        "SELECT * FROM a RIGHT JOIN b ON 1",
        "SELECT a REGEXP 'x'",
        "SELECT a -> 'x'",
        "CREATE TABLE t (a REFERENCES u(id))",
        "CREATE TRIGGER x",
        "CREATE VIEW v AS SELECT 1",
        "CREATE INDEX i ON t (a) WHERE a > 1",
        "SELECT row_number() OVER ()",
    ] {
        assert!(
            matches!(parse_one(sql), Err(Error::Unsupported(_))),
            "{sql}"
        );
    }
}

/// Statements both we and the stock `sqlite3` accept (against a database that has tables `t` and `u`).
const VALID: &[&str] = &[
    "SELECT 1",
    "SELECT 1 + 2 * 3 - 4 / 5 % 6",
    "SELECT a, b AS x, c y FROM t",
    "SELECT * FROM t WHERE a = 1 AND (b < 2 OR c >= 3)",
    "SELECT t.*, u.a FROM t JOIN u ON t.a = u.a",
    "SELECT * FROM t LEFT JOIN u USING (a)",
    "SELECT * FROM t CROSS JOIN u",
    "SELECT * FROM t, u WHERE t.a = u.a",
    "SELECT a, count(*), sum(b), min(c), max(c), avg(b) FROM t GROUP BY a HAVING count(*) > 1 ORDER BY 2 DESC, a",
    "SELECT DISTINCT a FROM t ORDER BY a LIMIT 5 OFFSET 2",
    "SELECT a FROM t LIMIT 2, 3",
    "SELECT a FROM t WHERE b IN (1, 2, 3) AND c NOT IN (SELECT a FROM u)",
    "SELECT a FROM t WHERE b BETWEEN 1 AND 5 AND c NOT BETWEEN 2 AND 3",
    "SELECT a FROM t WHERE b LIKE 'x%' ESCAPE '\\' OR c GLOB 'a*'",
    "SELECT a FROM t WHERE b IS NULL OR c IS NOT NULL OR a ISNULL OR b NOTNULL OR c NOT NULL",
    "SELECT a IS b, a IS NOT b, a IS DISTINCT FROM b FROM t",
    "SELECT CASE WHEN a > 1 THEN 'big' WHEN a > 0 THEN 'small' ELSE 'none' END FROM t",
    "SELECT CASE a WHEN 1 THEN 'one' END FROM t",
    "SELECT CAST(a AS INTEGER), CAST(b AS TEXT), CAST(c AS REAL) FROM t",
    "SELECT EXISTS (SELECT 1 FROM u WHERE u.a = t.a), (SELECT max(a) FROM u) FROM t",
    "SELECT coalesce(a, 0), ifnull(b, 1), nullif(c, 2), abs(-3), length('x'), upper(a), lower(b) FROM t",
    "SELECT a || b || 'x' FROM t",
    "SELECT ~a, -a, +a, a & b, a | b, a << 1, a >> 1 FROM t",
    "SELECT a FROM t WHERE NOT a = 1",
    "SELECT a FROM t WHERE NOT EXISTS (SELECT 1 FROM u)",
    "SELECT \"a\", [b], `c` FROM t",
    "SELECT x'ff', 0x10, 1e3, .5, 'it''s'",
    "SELECT a FROM t UNION SELECT a FROM u",
    "SELECT a FROM t UNION ALL SELECT a FROM u ORDER BY 1",
    "SELECT a FROM t INTERSECT SELECT a FROM u",
    "SELECT a FROM t EXCEPT SELECT a FROM u",
    "SELECT a FROM (SELECT a FROM t) AS s WHERE a > 1",
    "SELECT ?, ?2, :x, @y, $z",
    "SELECT a FROM t ORDER BY a NULLS FIRST, b DESC NULLS LAST",
    "SELECT a FROM t WHERE a = 'x' COLLATE NOCASE",
    "INSERT INTO t (a, b, c) VALUES (1, 2, 3), (4, 5, 6)",
    "INSERT INTO t VALUES (1, 2, 3)",
    "INSERT INTO t SELECT * FROM u",
    "INSERT OR REPLACE INTO t VALUES (1, 2, 3)",
    "INSERT OR IGNORE INTO t VALUES (1, 2, 3)",
    "REPLACE INTO t VALUES (1, 2, 3)",
    "INSERT INTO t DEFAULT VALUES",
    "INSERT INTO t VALUES (1, 2, 3) ON CONFLICT DO NOTHING",
    "INSERT INTO t (a, b) VALUES (1, 2) ON CONFLICT (a) DO UPDATE SET b = excluded.b + 1 WHERE b < 5",
    "INSERT INTO t VALUES (1, 2, 3) RETURNING a, b",
    "UPDATE t SET a = a + 1, b = 'x' WHERE c > 0",
    "UPDATE OR IGNORE t SET a = 1",
    "UPDATE t SET a = 1 RETURNING *",
    "DELETE FROM t WHERE a = 1",
    "DELETE FROM t",
    "DELETE FROM t WHERE a IN (SELECT a FROM u) RETURNING a",
    "CREATE TABLE x1 (id INTEGER PRIMARY KEY, name TEXT NOT NULL, n INT DEFAULT 0, r REAL, b BLOB)",
    "CREATE TABLE IF NOT EXISTS x2 (a TEXT, b TEXT, PRIMARY KEY (a, b))",
    "CREATE TABLE x3 (a INTEGER PRIMARY KEY AUTOINCREMENT, b UNIQUE, c CHECK (c > 0), d DEFAULT (1 + 2))",
    "CREATE TABLE x4 (a TEXT NOT NULL COLLATE NOCASE, b VARCHAR(255), c DECIMAL(10, 2), UNIQUE (a, b DESC))",
    "CREATE TABLE x5 (a, b, PRIMARY KEY (a)) WITHOUT ROWID",
    "CREATE TABLE x6 (a INT, b INT, CONSTRAINT ck CHECK (a < b))",
    "CREATE TABLE x7 (a UNSIGNED BIG INT, b DOUBLE PRECISION, c NATIVE CHARACTER(70))",
    "CREATE INDEX i1 ON t (a)",
    "CREATE UNIQUE INDEX IF NOT EXISTS i2 ON t (a DESC, b)",
    "DROP TABLE t",
    "DROP TABLE IF EXISTS nothere",
    "DROP INDEX IF EXISTS nothere",
    "ALTER TABLE t ADD COLUMN d TEXT DEFAULT 'x'",
    "ALTER TABLE t RENAME TO t2",
    "ALTER TABLE t RENAME COLUMN a TO z",
    "ALTER TABLE t DROP COLUMN c",
    "BEGIN",
    "BEGIN IMMEDIATE",
    "BEGIN EXCLUSIVE TRANSACTION",
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT sp",
    "PRAGMA user_version",
    "PRAGMA user_version = 7",
    "PRAGMA table_info(t)",
];

/// Statements that are wrong in SQL itself (both we and `sqlite3` refuse them).
const INVALID: &[&str] = &[
    "SELECT",
    "SELECT FROM t",
    "SELECT a FROM",
    "SELECT a FROM t WHERE",
    "SELECT a FROM t GROUP",
    "SELECT a FROM t ORDER a",
    "SELECT (1",
    "SELECT 1 +",
    "SELECT a b c FROM t",
    "SELECT * FROM t LEFT u",
    "SELECT a FROM t WHERE a IN",
    "SELECT a FROM t WHERE a BETWEEN 1",
    "SELECT CASE END",
    "SELECT CASE WHEN 1 END",
    "SELECT CAST(a)",
    "SELECT count(",
    "INSERT INTO t",
    "INSERT INTO t VALUES",
    "INSERT INTO t VALUES (1,)",
    "INSERT t VALUES (1)",
    "UPDATE t",
    "UPDATE t SET",
    "UPDATE t SET a",
    "DELETE t",
    "DELETE FROM",
    "CREATE TABLE",
    "CREATE TABLE x ()",
    "CREATE TABLE x (a,)",
    "CREATE TABLE x (a INT PRIMARY)",
    "CREATE INDEX i ON t",
    "CREATE INDEX ON t (a)",
    "DROP",
    "DROP TABLE",
    "ALTER TABLE t",
    "BEGIN BEGIN",
    "SELECT 1 2",
    "SELECT 'abc",
    "SELECT \"abc",
    "SELECT 1 FROM t WHERE a = = 1",
    "SELEC 1",
    "SELECT 1;;; DROP",
];

fn sqlite3_accepts(statement: &str) -> Option<bool> {
    let setup = "CREATE TABLE t(a UNIQUE,b,c); CREATE TABLE u(a,b,c);";
    let out = Command::new("sqlite3")
        .arg(":memory:")
        .arg(format!("{setup} EXPLAIN {statement}"))
        .output()
        .ok()?;
    Some(out.status.success())
}

#[quench_test::test]
fn every_statement_of_the_valid_corpus_parses_and_sqlite3_agrees_it_is_valid() {
    for sql in VALID {
        parse_one(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
        // `PRAGMA`s and a few others do not EXPLAIN meaningfully; ask sqlite3 anyway, the corpus is chosen to work.
        if let Some(ok) = sqlite3_accepts(sql) {
            assert!(ok, "sqlite3 refuses {sql}: the corpus is wrong");
        }
    }
}

#[quench_test::test]
fn every_statement_of_the_invalid_corpus_is_refused_by_us_and_by_sqlite3() {
    for sql in INVALID {
        assert!(
            matches!(parse_one(sql), Err(Error::Syntax { .. })),
            "{sql}: expected a syntax error, got {:?}",
            parse_one(sql)
        );
        if let Some(ok) = sqlite3_accepts(sql) {
            assert!(!ok, "sqlite3 accepts {sql}: the corpus is wrong");
        }
    }
}

#[quench_test::test]
fn random_garbage_never_panics_the_parser() {
    let alphabet: Vec<&str> = vec![
        "SELECT", "FROM", "WHERE", "(", ")", ",", "a", "b", "1", "2.5", "'x'", "\"q\"", "=", "<",
        "AND", "OR", "NOT", "IN", "NULL", "*", "+", "-", ";", "INSERT", "INTO", "VALUES", "CREATE",
        "TABLE", "JOIN", "ON", "CASE", "WHEN", "THEN", "END", "?", ":p", "--", "/*", "x'0", "[",
        "BETWEEN", "LIKE", "ORDER", "BY", "LIMIT", "AS",
    ];
    let mut state = 0x1234_5678_9abc_def0_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..20_000 {
        let n = (next() % 14) as usize;
        let sql: Vec<&str> = (0..n)
            .map(|_| alphabet[(next() % alphabet.len() as u64) as usize])
            .collect();
        let _ = parse(&sql.join(" "));
    }
}
