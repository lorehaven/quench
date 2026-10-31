//! Tokens to a syntax tree, by recursive descent; expressions by precedence climbing in SQLite's order.

use super::ast::{
    AlterAction, BinaryOp, ColumnConstraint, ColumnDef, CompoundOp, CreateIndex, CreateTable,
    Delete, Expr, From, IndexedColumn, Insert, InsertSource, Join, JoinConstraint, JoinKind,
    LikeKind, NullsOrder, OnConflict, OrderTerm, Param, ResultColumn, Select, SelectCore, Stmt,
    TableConstraint, TableSource, TransactionMode, UnaryOp, Update, Upsert, UpsertAction,
};
use super::lexer::{Kind, Token, tokenize};
use crate::error::{Error, Result};
use crate::record::Value;

/// Words that cannot be a bare name or alias: they start or continue a clause.
const RESERVED: &[&str] = &[
    "ADD",
    "ALL",
    "AND",
    "AS",
    "BETWEEN",
    "BY",
    "CASE",
    "CAST",
    "COLLATE",
    "CREATE",
    "CROSS",
    "DEFAULT",
    "DELETE",
    "DISTINCT",
    "DROP",
    "ELSE",
    "END",
    "EXCEPT",
    "EXISTS",
    "FROM",
    "GLOB",
    "GROUP",
    "HAVING",
    "IN",
    "INNER",
    "INSERT",
    "INTERSECT",
    "INTO",
    "IS",
    "ISNULL",
    "JOIN",
    "LEFT",
    "LIKE",
    "LIMIT",
    "NATURAL",
    "NOT",
    "NOTNULL",
    "NULL",
    "ON",
    "OR",
    "ORDER",
    "OUTER",
    "RETURNING",
    "SELECT",
    "SET",
    "THEN",
    "UNION",
    "UPDATE",
    "USING",
    "VALUES",
    "WHEN",
    "WHERE",
    "RIGHT",
    "FULL",
];

/// Parses every statement in `sql`.
pub fn parse(sql: &str) -> Result<Vec<Stmt>> {
    let mut p = Parser {
        toks: tokenize(sql)?,
        pos: 0,
        params: 0,
    };
    let mut out = Vec::new();
    loop {
        while p.accept_op(";") {}
        if p.at_eof() {
            return Ok(out);
        }
        p.params = 0;
        out.push(p.statement()?);
        if !p.at_eof() && !p.accept_op(";") {
            return Err(p.error("expected the end of the statement"));
        }
    }
}

/// Parses every statement in `sql`, each with its own text (without the `;`).
pub fn parse_with_text(sql: &str) -> Result<Vec<(Stmt, String)>> {
    let mut p = Parser {
        toks: tokenize(sql)?,
        pos: 0,
        params: 0,
    };
    let mut out = Vec::new();
    loop {
        while p.accept_op(";") {}
        if p.at_eof() {
            return Ok(out);
        }
        p.params = 0;
        let start = p.toks[p.pos].start;
        let stmt = p.statement()?;
        let end = p.toks[p.pos].start;
        out.push((stmt, sql[start..end].trim_end().to_string()));
        if !p.at_eof() && !p.accept_op(";") {
            return Err(p.error("expected the end of the statement"));
        }
    }
}

/// Parses `sql`, which must hold exactly one statement.
pub fn parse_one(sql: &str) -> Result<Stmt> {
    let mut all = parse(sql)?;
    match all.len() {
        1 => Ok(all.remove(0)),
        0 => Err(Error::Syntax {
            pos: 0,
            message: "there is no statement".into(),
        }),
        _ => Err(Error::Syntax {
            pos: 0,
            message: "expected one statement, found several".into(),
        }),
    }
}

struct Parser {
    toks: Vec<Token>,
    pos: usize,
    /// The largest parameter index so far (a plain `?` takes the next).
    params: usize,
}

fn unsupported<T>(what: &str) -> Result<T> {
    Err(Error::Unsupported(what.into()))
}

impl Parser {
    // ---- token access

    fn peek(&self) -> &Kind {
        &self.toks[self.pos].kind
    }

    fn peek_at(&self, ahead: usize) -> &Kind {
        &self.toks[(self.pos + ahead).min(self.toks.len() - 1)].kind
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Kind::Eof)
    }

    fn bump(&mut self) -> Kind {
        let kind = self.toks[self.pos].kind.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        kind
    }

    fn error(&self, message: &str) -> Error {
        let near = match self.peek() {
            Kind::Eof => "the end of the text".to_string(),
            Kind::Word { text, .. } => format!("{text:?}"),
            Kind::Op(op) => format!("{op:?}"),
            Kind::Integer(n) => n.to_string(),
            Kind::Float(n) => n.to_string(),
            Kind::Text(_) => "a string".into(),
            Kind::Blob(_) => "a blob".into(),
            Kind::Param(p) => p.clone(),
        };
        Error::Syntax {
            pos: self.toks[self.pos].start,
            message: format!("{message}, near {near}"),
        }
    }

    fn is_kw_at(&self, ahead: usize, kw: &str) -> bool {
        matches!(self.peek_at(ahead), Kind::Word { text, quoted: false } if text.eq_ignore_ascii_case(kw))
    }

    fn is_kw(&self, kw: &str) -> bool {
        self.is_kw_at(0, kw)
    }

    fn accept_kw(&mut self, kw: &str) -> bool {
        let yes = self.is_kw(kw);
        if yes {
            self.bump();
        }
        yes
    }

    fn expect_kw(&mut self, kw: &str) -> Result<()> {
        if self.accept_kw(kw) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {kw}")))
        }
    }

    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Kind::Op(o) if *o == op)
    }

    fn accept_op(&mut self, op: &str) -> bool {
        let yes = self.is_op(op);
        if yes {
            self.bump();
        }
        yes
    }

    fn expect_op(&mut self, op: &str) -> Result<()> {
        if self.accept_op(op) {
            Ok(())
        } else {
            Err(self.error(&format!("expected {op:?}")))
        }
    }

    /// A name: quoted, or bare and not a reserved word.
    fn ident(&mut self) -> Result<String> {
        match self.peek().clone() {
            Kind::Word { text, quoted } if quoted || !is_reserved(&text) => {
                self.bump();
                Ok(text)
            }
            _ => Err(self.error("expected a name")),
        }
    }

    fn is_name(&self) -> bool {
        matches!(self.peek(), Kind::Word { text, quoted } if *quoted || !is_reserved(text))
    }

    // ---- statements

    fn statement(&mut self) -> Result<Stmt> {
        let Kind::Word {
            text,
            quoted: false,
        } = self.peek().clone()
        else {
            return Err(self.error("expected a statement"));
        };
        match text.to_ascii_uppercase().as_str() {
            "SELECT" | "WITH" | "VALUES" => Ok(Stmt::Select(self.select()?)),
            "INSERT" | "REPLACE" => self.insert(),
            "UPDATE" => self.update(),
            "DELETE" => self.delete(),
            "CREATE" => self.create(),
            "DROP" => self.drop_stmt(),
            "ALTER" => self.alter(),
            "BEGIN" => {
                self.bump();
                let mode = if self.accept_kw("IMMEDIATE") {
                    TransactionMode::Immediate
                } else if self.accept_kw("EXCLUSIVE") {
                    TransactionMode::Exclusive
                } else {
                    self.accept_kw("DEFERRED");
                    TransactionMode::Deferred
                };
                self.accept_kw("TRANSACTION");
                Ok(Stmt::Begin(mode))
            }
            "COMMIT" | "END" => {
                self.bump();
                self.accept_kw("TRANSACTION");
                Ok(Stmt::Commit)
            }
            "ROLLBACK" => {
                self.bump();
                self.accept_kw("TRANSACTION");
                if self.accept_kw("TO") {
                    self.accept_kw("SAVEPOINT");
                    return Ok(Stmt::RollbackTo(self.ident()?));
                }
                Ok(Stmt::Rollback)
            }
            "SAVEPOINT" => {
                self.bump();
                Ok(Stmt::Savepoint(self.ident()?))
            }
            "RELEASE" => {
                self.bump();
                self.accept_kw("SAVEPOINT");
                Ok(Stmt::Release(self.ident()?))
            }
            "PRAGMA" => self.pragma(),
            other => Err(self.error(&format!("unknown statement {other}"))),
        }
    }

    fn pragma(&mut self) -> Result<Stmt> {
        self.bump();
        let first = self.ident()?;
        // `main.user_version`: only the main database exists.
        let name = if self.accept_op(".") {
            self.ident()?
        } else {
            first
        };
        let value = if self.accept_op("=") {
            Some(self.pragma_value()?)
        } else if self.accept_op("(") {
            let v = self.pragma_value()?;
            self.expect_op(")")?;
            Some(v)
        } else {
            None
        };
        Ok(Stmt::Pragma { name, value })
    }

    fn pragma_value(&mut self) -> Result<Value> {
        let negative = self.accept_op("-");
        if !negative {
            self.accept_op("+");
        }
        match self.bump() {
            Kind::Integer(n) => Ok(Value::Integer(if negative { -n } else { n })),
            Kind::Float(n) => Ok(Value::Real(if negative { -n } else { n })),
            Kind::Text(s) | Kind::Word { text: s, .. } if !negative => Ok(Value::Text(s)),
            _ => {
                self.pos -= 1;
                Err(self.error("expected a pragma value"))
            }
        }
    }

    // ---- SELECT

    fn select(&mut self) -> Result<Select> {
        if self.is_kw("WITH") {
            return unsupported("WITH (common table expressions)");
        }
        if self.is_kw("VALUES") {
            return unsupported("VALUES as a query");
        }
        let core = self.select_core()?;
        let mut compounds = Vec::new();
        loop {
            let op = if self.accept_kw("UNION") {
                if self.accept_kw("ALL") {
                    CompoundOp::UnionAll
                } else {
                    CompoundOp::Union
                }
            } else if self.accept_kw("INTERSECT") {
                CompoundOp::Intersect
            } else if self.accept_kw("EXCEPT") {
                CompoundOp::Except
            } else {
                break;
            };
            compounds.push((op, self.select_core()?));
        }
        let mut order_by = Vec::new();
        if self.accept_kw("ORDER") {
            self.expect_kw("BY")?;
            loop {
                order_by.push(self.order_term()?);
                if !self.accept_op(",") {
                    break;
                }
            }
        }
        let (mut limit, mut offset) = (None, None);
        if self.accept_kw("LIMIT") {
            limit = Some(self.expr()?);
            if self.accept_kw("OFFSET") {
                offset = Some(self.expr()?);
            } else if self.accept_op(",") {
                // `LIMIT offset, count`
                offset = limit.take();
                limit = Some(self.expr()?);
            }
        }
        Ok(Select {
            core,
            compounds,
            order_by,
            limit,
            offset,
        })
    }

    fn order_term(&mut self) -> Result<OrderTerm> {
        let expr = self.expr()?;
        let descending = if self.accept_kw("DESC") {
            true
        } else {
            self.accept_kw("ASC");
            false
        };
        let nulls = if self.accept_kw("NULLS") {
            if self.accept_kw("FIRST") {
                NullsOrder::First
            } else {
                self.expect_kw("LAST")?;
                NullsOrder::Last
            }
        } else {
            NullsOrder::Default
        };
        Ok(OrderTerm {
            expr,
            descending,
            nulls,
        })
    }

    fn select_core(&mut self) -> Result<SelectCore> {
        self.expect_kw("SELECT")?;
        let distinct = if self.accept_kw("DISTINCT") {
            true
        } else {
            self.accept_kw("ALL");
            false
        };
        let columns = self.result_columns()?;
        let from = if self.accept_kw("FROM") {
            Some(self.parse_from()?)
        } else {
            None
        };
        let where_clause = if self.accept_kw("WHERE") {
            Some(self.expr()?)
        } else {
            None
        };
        let mut group_by = Vec::new();
        if self.accept_kw("GROUP") {
            self.expect_kw("BY")?;
            loop {
                group_by.push(self.expr()?);
                if !self.accept_op(",") {
                    break;
                }
            }
        }
        let having = if self.accept_kw("HAVING") {
            Some(self.expr()?)
        } else {
            None
        };
        if self.is_kw("WINDOW") {
            return unsupported("window functions");
        }
        Ok(SelectCore {
            distinct,
            columns,
            from,
            where_clause,
            group_by,
            having,
        })
    }

    fn result_columns(&mut self) -> Result<Vec<ResultColumn>> {
        let mut out = Vec::new();
        loop {
            if self.accept_op("*") {
                out.push(ResultColumn::All);
            } else if self.is_name()
                && matches!(self.peek_at(1), Kind::Op("."))
                && matches!(self.peek_at(2), Kind::Op("*"))
            {
                let table = self.ident()?;
                self.bump();
                self.bump();
                out.push(ResultColumn::TableAll(table));
            } else {
                let expr = self.expr()?;
                out.push(ResultColumn::Expr {
                    expr,
                    alias: self.alias()?,
                });
            }
            if !self.accept_op(",") {
                return Ok(out);
            }
        }
    }

    fn alias(&mut self) -> Result<Option<String>> {
        if self.accept_kw("AS") {
            return match self.peek().clone() {
                Kind::Text(s) => {
                    self.bump();
                    Ok(Some(s))
                }
                _ => Ok(Some(self.ident()?)),
            };
        }
        if self.is_name() && !self.is_kw("WINDOW") {
            return Ok(Some(self.ident()?));
        }
        Ok(None)
    }

    fn parse_from(&mut self) -> Result<From> {
        let first = self.table_source()?;
        let mut joins = Vec::new();
        loop {
            let kind = if self.accept_op(",") {
                joins.push(Join {
                    kind: JoinKind::Cross,
                    source: self.table_source()?,
                    constraint: JoinConstraint::None,
                });
                continue;
            } else if self.is_kw("NATURAL") {
                return unsupported("NATURAL JOIN");
            } else if self.accept_kw("LEFT") {
                self.accept_kw("OUTER");
                JoinKind::Left
            } else if self.is_kw("RIGHT") || self.is_kw("FULL") {
                return unsupported("RIGHT and FULL joins");
            } else if self.accept_kw("INNER") {
                JoinKind::Inner
            } else if self.accept_kw("CROSS") {
                JoinKind::Cross
            } else if self.is_kw("JOIN") {
                JoinKind::Inner
            } else {
                return Ok(From { first, joins });
            };
            self.expect_kw("JOIN")?;
            let source = self.table_source()?;
            let constraint = if self.accept_kw("ON") {
                JoinConstraint::On(self.expr()?)
            } else if self.accept_kw("USING") {
                self.expect_op("(")?;
                let mut cols = vec![self.ident()?];
                while self.accept_op(",") {
                    cols.push(self.ident()?);
                }
                self.expect_op(")")?;
                JoinConstraint::Using(cols)
            } else {
                JoinConstraint::None
            };
            joins.push(Join {
                kind,
                source,
                constraint,
            });
        }
    }

    fn table_source(&mut self) -> Result<TableSource> {
        if self.accept_op("(") {
            if !self.is_kw("SELECT") {
                return unsupported("a parenthesised join or VALUES in FROM");
            }
            let select = Box::new(self.select()?);
            self.expect_op(")")?;
            return Ok(TableSource::Subquery {
                select,
                alias: self.alias()?,
            });
        }
        let name = self.qualified_name()?;
        if self.is_op("(") {
            return unsupported("table-valued functions");
        }
        Ok(TableSource::Table {
            name,
            alias: self.alias()?,
        })
    }

    /// `name` or `main.name`; only the main database exists.
    fn qualified_name(&mut self) -> Result<String> {
        let first = self.ident()?;
        if self.accept_op(".") {
            let second = self.ident()?;
            if !first.eq_ignore_ascii_case("main") {
                return unsupported("attached databases");
            }
            return Ok(second);
        }
        Ok(first)
    }

    // ---- INSERT, UPDATE, DELETE

    fn conflict_word(&mut self) -> Result<OnConflict> {
        for (word, kind) in [
            ("ABORT", OnConflict::Abort),
            ("REPLACE", OnConflict::Replace),
            ("IGNORE", OnConflict::Ignore),
            ("FAIL", OnConflict::Fail),
            ("ROLLBACK", OnConflict::Rollback),
        ] {
            if self.accept_kw(word) {
                return Ok(kind);
            }
        }
        Err(self.error("expected ROLLBACK, ABORT, FAIL, IGNORE or REPLACE"))
    }

    fn insert(&mut self) -> Result<Stmt> {
        let or = if self.accept_kw("REPLACE") {
            OnConflict::Replace
        } else {
            self.expect_kw("INSERT")?;
            if self.accept_kw("OR") {
                self.conflict_word()?
            } else {
                OnConflict::Abort
            }
        };
        self.expect_kw("INTO")?;
        let table = self.qualified_name()?;
        let columns = if self.accept_op("(") {
            let mut cols = vec![self.ident()?];
            while self.accept_op(",") {
                cols.push(self.ident()?);
            }
            self.expect_op(")")?;
            Some(cols)
        } else {
            None
        };
        let source = if self.accept_kw("DEFAULT") {
            self.expect_kw("VALUES")?;
            InsertSource::DefaultValues
        } else if self.accept_kw("VALUES") {
            let mut rows = Vec::new();
            loop {
                self.expect_op("(")?;
                let mut row = vec![self.expr()?];
                while self.accept_op(",") {
                    row.push(self.expr()?);
                }
                self.expect_op(")")?;
                rows.push(row);
                if !self.accept_op(",") {
                    break;
                }
            }
            InsertSource::Values(rows)
        } else if self.is_kw("SELECT") {
            InsertSource::Select(Box::new(self.select()?))
        } else {
            return Err(self.error("expected VALUES, SELECT or DEFAULT VALUES"));
        };
        let upsert = if self.is_kw("ON") {
            Some(self.upsert()?)
        } else {
            None
        };
        Ok(Stmt::Insert(Insert {
            or,
            table,
            columns,
            source,
            upsert,
            returning: self.returning()?,
        }))
    }

    fn upsert(&mut self) -> Result<Upsert> {
        self.expect_kw("ON")?;
        self.expect_kw("CONFLICT")?;
        let mut target = Vec::new();
        if self.accept_op("(") {
            target.push(self.ident()?);
            while self.accept_op(",") {
                target.push(self.ident()?);
            }
            self.expect_op(")")?;
            if self.is_kw("WHERE") {
                return unsupported("a WHERE on an upsert target (partial indexes)");
            }
        }
        self.expect_kw("DO")?;
        if self.accept_kw("NOTHING") {
            return Ok(Upsert {
                target,
                action: UpsertAction::Nothing,
            });
        }
        self.expect_kw("UPDATE")?;
        self.expect_kw("SET")?;
        let sets = self.assignments()?;
        let where_clause = if self.accept_kw("WHERE") {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Upsert {
            target,
            action: UpsertAction::Update { sets, where_clause },
        })
    }

    fn assignments(&mut self) -> Result<Vec<(String, Expr)>> {
        let mut sets = Vec::new();
        loop {
            if self.is_op("(") {
                return unsupported("(a, b) = (...) assignments");
            }
            let name = self.ident()?;
            self.expect_op("=")?;
            sets.push((name, self.expr()?));
            if !self.accept_op(",") {
                return Ok(sets);
            }
        }
    }

    fn returning(&mut self) -> Result<Vec<ResultColumn>> {
        if self.accept_kw("RETURNING") {
            self.result_columns()
        } else {
            Ok(Vec::new())
        }
    }

    fn update(&mut self) -> Result<Stmt> {
        self.expect_kw("UPDATE")?;
        let or = if self.accept_kw("OR") {
            self.conflict_word()?
        } else {
            OnConflict::Abort
        };
        let table = self.qualified_name()?;
        self.expect_kw("SET")?;
        let sets = self.assignments()?;
        if self.is_kw("FROM") {
            return unsupported("UPDATE ... FROM");
        }
        let where_clause = if self.accept_kw("WHERE") {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Stmt::Update(Update {
            or,
            table,
            sets,
            where_clause,
            returning: self.returning()?,
        }))
    }

    fn delete(&mut self) -> Result<Stmt> {
        self.expect_kw("DELETE")?;
        self.expect_kw("FROM")?;
        let table = self.qualified_name()?;
        let where_clause = if self.accept_kw("WHERE") {
            Some(self.expr()?)
        } else {
            None
        };
        Ok(Stmt::Delete(Delete {
            table,
            where_clause,
            returning: self.returning()?,
        }))
    }

    // ---- CREATE, DROP, ALTER

    fn create(&mut self) -> Result<Stmt> {
        self.expect_kw("CREATE")?;
        if self.is_kw("TEMP") || self.is_kw("TEMPORARY") {
            return unsupported("temporary tables");
        }
        let unique = self.accept_kw("UNIQUE");
        if self.accept_kw("INDEX") {
            return self.create_index(unique);
        }
        if unique {
            return Err(self.error("expected INDEX"));
        }
        if self.is_kw("VIEW") || self.is_kw("TRIGGER") || self.is_kw("VIRTUAL") {
            return unsupported("views, triggers and virtual tables");
        }
        self.expect_kw("TABLE")?;
        self.create_table()
    }

    fn if_not_exists(&mut self) -> Result<bool> {
        if self.accept_kw("IF") {
            self.expect_kw("NOT")?;
            self.expect_kw("EXISTS")?;
            return Ok(true);
        }
        Ok(false)
    }

    fn if_exists(&mut self) -> Result<bool> {
        if self.accept_kw("IF") {
            self.expect_kw("EXISTS")?;
            return Ok(true);
        }
        Ok(false)
    }

    fn create_index(&mut self, unique: bool) -> Result<Stmt> {
        let if_not_exists = self.if_not_exists()?;
        let name = self.qualified_name()?;
        self.expect_kw("ON")?;
        let table = self.qualified_name()?;
        self.expect_op("(")?;
        let columns = self.indexed_columns()?;
        if self.is_kw("WHERE") {
            return unsupported("partial indexes");
        }
        Ok(Stmt::CreateIndex(CreateIndex {
            name,
            table,
            unique,
            if_not_exists,
            columns,
        }))
    }

    /// Columns after an opening `(`, through the closing `)`.
    fn indexed_columns(&mut self) -> Result<Vec<IndexedColumn>> {
        let mut cols = Vec::new();
        loop {
            let name = self.ident()?;
            if self.is_op("(") || self.is_op(".") {
                return unsupported("expressions in an index");
            }
            if self.accept_kw("COLLATE") {
                self.ident()?;
            }
            let descending = if self.accept_kw("DESC") {
                true
            } else {
                self.accept_kw("ASC");
                false
            };
            cols.push(IndexedColumn { name, descending });
            if !self.accept_op(",") {
                break;
            }
        }
        self.expect_op(")")?;
        Ok(cols)
    }

    fn create_table(&mut self) -> Result<Stmt> {
        let if_not_exists = self.if_not_exists()?;
        let name = self.qualified_name()?;
        if self.is_kw("AS") {
            return unsupported("CREATE TABLE ... AS SELECT");
        }
        self.expect_op("(")?;
        let mut columns = Vec::new();
        let mut constraints = Vec::new();
        loop {
            if self.is_kw("CONSTRAINT")
                || self.is_kw("PRIMARY")
                || self.is_kw("UNIQUE")
                || self.is_kw("CHECK")
                || self.is_kw("FOREIGN")
            {
                constraints.push(self.table_constraint()?);
            } else {
                columns.push(self.column_def()?);
            }
            if !self.accept_op(",") {
                break;
            }
        }
        self.expect_op(")")?;
        let without_rowid = self.accept_kw("WITHOUT");
        if without_rowid {
            self.expect_kw("ROWID")?;
        }
        if columns.is_empty() {
            return Err(self.error("a table needs a column"));
        }
        Ok(Stmt::CreateTable(CreateTable {
            name,
            if_not_exists,
            columns,
            constraints,
            without_rowid,
        }))
    }

    fn table_constraint(&mut self) -> Result<TableConstraint> {
        if self.accept_kw("CONSTRAINT") {
            self.ident()?;
        }
        if self.accept_kw("PRIMARY") {
            self.expect_kw("KEY")?;
            self.expect_op("(")?;
            let cols = self.indexed_columns()?;
            self.no_conflict_clause()?;
            return Ok(TableConstraint::PrimaryKey(cols));
        }
        if self.accept_kw("UNIQUE") {
            self.expect_op("(")?;
            let cols = self.indexed_columns()?;
            self.no_conflict_clause()?;
            return Ok(TableConstraint::Unique(cols));
        }
        if self.accept_kw("CHECK") {
            self.expect_op("(")?;
            let e = self.expr()?;
            self.expect_op(")")?;
            return Ok(TableConstraint::Check(e));
        }
        if self.is_kw("FOREIGN") {
            return unsupported("foreign keys");
        }
        Err(self.error("expected a table constraint"))
    }

    fn no_conflict_clause(&self) -> Result<()> {
        if self.is_kw("ON") && self.is_kw_at(1, "CONFLICT") {
            return unsupported("a conflict clause on a constraint");
        }
        Ok(())
    }

    fn column_def(&mut self) -> Result<ColumnDef> {
        let name = self.ident()?;
        let type_name = self.type_name()?;
        let mut constraints = Vec::new();
        loop {
            if self.accept_kw("CONSTRAINT") {
                self.ident()?;
            }
            if self.accept_kw("PRIMARY") {
                self.expect_kw("KEY")?;
                let descending = if self.accept_kw("DESC") {
                    true
                } else {
                    self.accept_kw("ASC");
                    false
                };
                self.no_conflict_clause()?;
                let autoincrement = self.accept_kw("AUTOINCREMENT");
                constraints.push(ColumnConstraint::PrimaryKey {
                    descending,
                    autoincrement,
                });
            } else if self.accept_kw("NOT") {
                self.expect_kw("NULL")?;
                self.no_conflict_clause()?;
                constraints.push(ColumnConstraint::NotNull);
            } else if self.accept_kw("NULL") {
                self.no_conflict_clause()?;
            } else if self.accept_kw("UNIQUE") {
                self.no_conflict_clause()?;
                constraints.push(ColumnConstraint::Unique);
            } else if self.accept_kw("CHECK") {
                self.expect_op("(")?;
                let e = self.expr()?;
                self.expect_op(")")?;
                constraints.push(ColumnConstraint::Check(e));
            } else if self.accept_kw("DEFAULT") {
                let e = if self.accept_op("(") {
                    let e = self.expr()?;
                    self.expect_op(")")?;
                    e
                } else {
                    self.expr_bp(PREFIX)?
                };
                constraints.push(ColumnConstraint::Default(e));
            } else if self.accept_kw("COLLATE") {
                constraints.push(ColumnConstraint::Collate(self.ident()?));
            } else if self.is_kw("REFERENCES") {
                return unsupported("foreign keys");
            } else if self.is_kw("GENERATED") || self.is_kw("AS") {
                return unsupported("generated columns");
            } else {
                return Ok(ColumnDef {
                    name,
                    type_name,
                    constraints,
                });
            }
        }
    }

    /// A declared type: words (and quoted names) up to a constraint or the end of the column, with an optional
    /// `(n)` or `(n, m)`.
    fn type_name(&mut self) -> Result<Option<String>> {
        const STOPPERS: [&str; 10] = [
            "PRIMARY",
            "NOT",
            "NULL",
            "UNIQUE",
            "CHECK",
            "DEFAULT",
            "COLLATE",
            "REFERENCES",
            "CONSTRAINT",
            "GENERATED",
        ];
        let mut parts: Vec<String> = Vec::new();
        while let Kind::Word { text, quoted } = self.peek().clone() {
            if !quoted
                && (STOPPERS.iter().any(|s| text.eq_ignore_ascii_case(s))
                    || text.eq_ignore_ascii_case("AS"))
            {
                break;
            }
            self.bump();
            parts.push(text);
        }
        if parts.is_empty() {
            return Ok(None);
        }
        let mut name = parts.join(" ");
        if self.accept_op("(") {
            let mut args = Vec::new();
            loop {
                let negative = self.accept_op("-");
                match self.bump() {
                    Kind::Integer(n) => args.push(if negative {
                        format!("-{n}")
                    } else {
                        n.to_string()
                    }),
                    Kind::Float(n) => args.push(n.to_string()),
                    _ => {
                        self.pos -= 1;
                        return Err(self.error("expected a number in the type"));
                    }
                }
                if !self.accept_op(",") {
                    break;
                }
            }
            self.expect_op(")")?;
            name = format!("{name}({})", args.join(","));
        }
        Ok(Some(name))
    }

    fn drop_stmt(&mut self) -> Result<Stmt> {
        self.expect_kw("DROP")?;
        if self.accept_kw("TABLE") {
            let if_exists = self.if_exists()?;
            return Ok(Stmt::DropTable {
                name: self.qualified_name()?,
                if_exists,
            });
        }
        if self.accept_kw("INDEX") {
            let if_exists = self.if_exists()?;
            return Ok(Stmt::DropIndex {
                name: self.qualified_name()?,
                if_exists,
            });
        }
        if self.is_kw("VIEW") || self.is_kw("TRIGGER") {
            return unsupported("views and triggers");
        }
        Err(self.error("expected TABLE or INDEX"))
    }

    fn alter(&mut self) -> Result<Stmt> {
        self.expect_kw("ALTER")?;
        self.expect_kw("TABLE")?;
        let table = self.qualified_name()?;
        let action = if self.accept_kw("ADD") {
            self.accept_kw("COLUMN");
            AlterAction::AddColumn(self.column_def()?)
        } else if self.accept_kw("RENAME") {
            if self.accept_kw("TO") {
                AlterAction::RenameTable(self.ident()?)
            } else {
                self.accept_kw("COLUMN");
                let from = self.ident()?;
                self.expect_kw("TO")?;
                AlterAction::RenameColumn {
                    from,
                    to: self.ident()?,
                }
            }
        } else if self.accept_kw("DROP") {
            self.accept_kw("COLUMN");
            AlterAction::DropColumn(self.ident()?)
        } else {
            return Err(self.error("expected ADD, RENAME or DROP"));
        };
        Ok(Stmt::AlterTable { table, action })
    }

    // ---- expressions

    fn expr(&mut self) -> Result<Expr> {
        self.expr_bp(0)
    }

    fn expr_bp(&mut self, min: u8) -> Result<Expr> {
        let mut left = self.prefix()?;
        while let Some(prec) = self.infix_precedence() {
            if prec < min {
                break;
            }
            left = self.infix(left, prec)?;
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr> {
        if self.accept_op("-") {
            let operand = self.expr_bp(PREFIX)?;
            return Ok(match operand {
                Expr::Literal(Value::Integer(n)) if n != i64::MIN => {
                    Expr::Literal(Value::Integer(-n))
                }
                Expr::Literal(Value::Real(n)) => Expr::Literal(Value::Real(-n)),
                other => Expr::Unary(UnaryOp::Neg, Box::new(other)),
            });
        }
        if self.accept_op("+") {
            return Ok(Expr::Unary(UnaryOp::Plus, Box::new(self.expr_bp(PREFIX)?)));
        }
        if self.accept_op("~") {
            return Ok(Expr::Unary(
                UnaryOp::BitNot,
                Box::new(self.expr_bp(PREFIX)?),
            ));
        }
        if self.is_kw("NOT") && !self.is_kw_at(1, "EXISTS") {
            self.bump();
            return Ok(Expr::Unary(UnaryOp::Not, Box::new(self.expr_bp(EQUALITY)?)));
        }
        self.primary()
    }

    fn param(&mut self, text: &str) -> Result<Param> {
        if let Some(digits) = text.strip_prefix('?') {
            if digits.is_empty() {
                self.params += 1;
                return Ok(Param::Index(self.params));
            }
            let n: usize = digits
                .parse()
                .map_err(|_| self.error("malformed parameter"))?;
            if n == 0 {
                return Err(self.error("parameter numbers start at 1"));
            }
            self.params = self.params.max(n);
            return Ok(Param::Index(n));
        }
        Ok(Param::Named(text[1..].to_string()))
    }

    fn primary(&mut self) -> Result<Expr> {
        match self.peek().clone() {
            Kind::Integer(n) => {
                self.bump();
                Ok(Expr::Literal(Value::Integer(n)))
            }
            Kind::Float(n) => {
                self.bump();
                Ok(Expr::Literal(Value::Real(n)))
            }
            Kind::Text(s) => {
                self.bump();
                Ok(Expr::Literal(Value::Text(s)))
            }
            Kind::Blob(b) => {
                self.bump();
                Ok(Expr::Literal(Value::Blob(b)))
            }
            Kind::Param(text) => {
                self.bump();
                Ok(Expr::Param(self.param(&text)?))
            }
            Kind::Op("(") => {
                self.bump();
                if self.is_kw("SELECT") || self.is_kw("WITH") {
                    let select = self.select()?;
                    self.expect_op(")")?;
                    return Ok(Expr::Subquery(Box::new(select)));
                }
                let e = self.expr()?;
                if self.is_op(",") {
                    return unsupported("row values");
                }
                self.expect_op(")")?;
                Ok(e)
            }
            Kind::Word { text, quoted } => self.word_expr(&text, quoted),
            _ => Err(self.error("expected an expression")),
        }
    }

    fn word_expr(&mut self, text: &str, quoted: bool) -> Result<Expr> {
        if !quoted {
            match text.to_ascii_uppercase().as_str() {
                "NULL" => {
                    self.bump();
                    return Ok(Expr::Literal(Value::Null));
                }
                "TRUE" => {
                    self.bump();
                    return Ok(Expr::Literal(Value::Integer(1)));
                }
                "FALSE" => {
                    self.bump();
                    return Ok(Expr::Literal(Value::Integer(0)));
                }
                "CURRENT_TIMESTAMP" | "CURRENT_DATE" | "CURRENT_TIME" => {
                    self.bump();
                    return Ok(Expr::Function {
                        name: text.to_ascii_lowercase(),
                        args: Vec::new(),
                        distinct: false,
                        star: false,
                    });
                }
                "CASE" => return self.case_expr(),
                "CAST" => return self.cast_expr(),
                "EXISTS" => {
                    self.bump();
                    return self.exists();
                }
                "NOT" if self.is_kw_at(1, "EXISTS") => {
                    self.bump();
                    self.bump();
                    let e = self.exists()?;
                    return Ok(Expr::Unary(UnaryOp::Not, Box::new(e)));
                }
                _ => {}
            }
            if is_reserved(text) {
                return Err(self.error("expected an expression"));
            }
        }
        self.bump();
        if self.accept_op("(") {
            return self.function_call(text);
        }
        if self.accept_op(".") {
            let name = self.ident()?;
            if self.is_op(".") {
                return unsupported("schema-qualified column names");
            }
            return Ok(Expr::Column {
                table: Some(text.to_string()),
                name,
            });
        }
        Ok(Expr::Column {
            table: None,
            name: text.to_string(),
        })
    }

    fn exists(&mut self) -> Result<Expr> {
        self.expect_op("(")?;
        let select = self.select()?;
        self.expect_op(")")?;
        Ok(Expr::Exists(Box::new(select)))
    }

    fn function_call(&mut self, name: &str) -> Result<Expr> {
        let name = name.to_ascii_lowercase();
        let mut args = Vec::new();
        let (mut distinct, mut star) = (false, false);
        if self.accept_op("*") {
            star = true;
        } else if !self.is_op(")") {
            distinct = self.accept_kw("DISTINCT");
            args.push(self.expr()?);
            while self.accept_op(",") {
                args.push(self.expr()?);
            }
        }
        self.expect_op(")")?;
        if self.is_kw("FILTER") || self.is_kw("OVER") {
            return unsupported("FILTER and OVER (window functions)");
        }
        Ok(Expr::Function {
            name,
            args,
            distinct,
            star,
        })
    }

    fn case_expr(&mut self) -> Result<Expr> {
        self.bump();
        let operand = if self.is_kw("WHEN") {
            None
        } else {
            Some(Box::new(self.expr()?))
        };
        let mut whens = Vec::new();
        while self.accept_kw("WHEN") {
            let when = self.expr()?;
            self.expect_kw("THEN")?;
            whens.push((when, self.expr()?));
        }
        if whens.is_empty() {
            return Err(self.error("CASE needs a WHEN"));
        }
        let otherwise = if self.accept_kw("ELSE") {
            Some(Box::new(self.expr()?))
        } else {
            None
        };
        self.expect_kw("END")?;
        Ok(Expr::Case {
            operand,
            whens,
            otherwise,
        })
    }

    fn cast_expr(&mut self) -> Result<Expr> {
        self.bump();
        self.expect_op("(")?;
        let expr = Box::new(self.expr()?);
        self.expect_kw("AS")?;
        let type_name = self
            .type_name()?
            .ok_or_else(|| self.error("expected a type"))?;
        self.expect_op(")")?;
        Ok(Expr::Cast { expr, type_name })
    }

    /// The precedence of the operator at the cursor, if one is there.
    fn infix_precedence(&self) -> Option<u8> {
        match self.peek() {
            Kind::Op(op) => Some(match *op {
                "||" | "->" | "->>" => CONCAT,
                "*" | "/" | "%" => MULTIPLY,
                "+" | "-" => ADD,
                "&" | "|" | "<<" | ">>" => BITWISE,
                "<" | "<=" | ">" | ">=" => COMPARE,
                "=" | "==" | "!=" | "<>" => EQUALITY,
                _ => return None,
            }),
            Kind::Word {
                text,
                quoted: false,
            } => match text.to_ascii_uppercase().as_str() {
                "OR" => Some(OR),
                "AND" => Some(AND),
                "IS" | "IN" | "LIKE" | "GLOB" | "BETWEEN" | "ISNULL" | "NOTNULL" | "REGEXP"
                | "MATCH" => Some(EQUALITY),
                "NOT" if self.not_infix() => Some(EQUALITY),
                "COLLATE" => Some(COLLATE),
                _ => None,
            },
            _ => None,
        }
    }

    /// `NOT` followed by what makes it an infix: `x NOT IN`, `NOT LIKE`, `NOT BETWEEN`, `NOT NULL`.
    fn not_infix(&self) -> bool {
        ["IN", "LIKE", "GLOB", "BETWEEN", "NULL", "REGEXP", "MATCH"]
            .iter()
            .any(|kw| self.is_kw_at(1, kw))
    }

    fn binary(&mut self, left: Expr, op: BinaryOp, prec: u8) -> Result<Expr> {
        self.bump();
        let right = self.expr_bp(prec + 1)?;
        Ok(Expr::Binary(op, Box::new(left), Box::new(right)))
    }

    fn infix(&mut self, left: Expr, prec: u8) -> Result<Expr> {
        match self.peek().clone() {
            Kind::Op(op) => {
                let op = match op {
                    "||" => BinaryOp::Concat,
                    "->" | "->>" => return unsupported("the JSON operators -> and ->>"),
                    "*" => BinaryOp::Mul,
                    "/" => BinaryOp::Div,
                    "%" => BinaryOp::Rem,
                    "+" => BinaryOp::Add,
                    "-" => BinaryOp::Sub,
                    "&" => BinaryOp::BitAnd,
                    "|" => BinaryOp::BitOr,
                    "<<" => BinaryOp::ShiftLeft,
                    ">>" => BinaryOp::ShiftRight,
                    "<" => BinaryOp::Lt,
                    "<=" => BinaryOp::Le,
                    ">" => BinaryOp::Gt,
                    ">=" => BinaryOp::Ge,
                    "=" | "==" => BinaryOp::Eq,
                    _ => BinaryOp::Ne,
                };
                self.binary(left, op, prec)
            }
            Kind::Word { text, .. } => match text.to_ascii_uppercase().as_str() {
                "OR" => self.binary(left, BinaryOp::Or, prec),
                "AND" => self.binary(left, BinaryOp::And, prec),
                "COLLATE" => {
                    self.bump();
                    Ok(Expr::Collate {
                        expr: Box::new(left),
                        collation: self.ident()?,
                    })
                }
                "ISNULL" => {
                    self.bump();
                    Ok(Expr::IsNull {
                        expr: Box::new(left),
                        negated: false,
                    })
                }
                "NOTNULL" => {
                    self.bump();
                    Ok(Expr::IsNull {
                        expr: Box::new(left),
                        negated: true,
                    })
                }
                "IS" => self.is_expr(left),
                "REGEXP" | "MATCH" => unsupported("REGEXP and MATCH"),
                _ => self.membership(left),
            },
            _ => Err(self.error("expected an operator")),
        }
    }

    fn is_expr(&mut self, left: Expr) -> Result<Expr> {
        self.bump();
        let mut negated = self.accept_kw("NOT");
        if self.accept_kw("DISTINCT") {
            self.expect_kw("FROM")?;
            negated = !negated;
        }
        if self.is_kw("NULL") && !self.is_kw_at(0, "DISTINCT") {
            self.bump();
            return Ok(Expr::IsNull {
                expr: Box::new(left),
                negated,
            });
        }
        let right = self.expr_bp(COMPARE)?;
        let op = if negated {
            BinaryOp::IsNot
        } else {
            BinaryOp::Is
        };
        Ok(Expr::Binary(op, Box::new(left), Box::new(right)))
    }

    /// `[NOT] IN`, `[NOT] BETWEEN`, `[NOT] LIKE`, `[NOT] GLOB`, `NOT NULL`.
    fn membership(&mut self, left: Expr) -> Result<Expr> {
        let negated = self.accept_kw("NOT");
        let expr = Box::new(left);
        if self.accept_kw("NULL") {
            return Ok(Expr::IsNull {
                expr,
                negated: true,
            });
        }
        if self.accept_kw("IN") {
            self.expect_op("(")?;
            if self.is_kw("SELECT") {
                let select = Box::new(self.select()?);
                self.expect_op(")")?;
                return Ok(Expr::InSelect {
                    expr,
                    select,
                    negated,
                });
            }
            let mut list = Vec::new();
            if !self.is_op(")") {
                list.push(self.expr()?);
                while self.accept_op(",") {
                    list.push(self.expr()?);
                }
            }
            self.expect_op(")")?;
            return Ok(Expr::InList {
                expr,
                list,
                negated,
            });
        }
        if self.accept_kw("BETWEEN") {
            let low = Box::new(self.expr_bp(COMPARE)?);
            self.expect_kw("AND")?;
            let high = Box::new(self.expr_bp(COMPARE)?);
            return Ok(Expr::Between {
                expr,
                low,
                high,
                negated,
            });
        }
        let kind = if self.accept_kw("LIKE") {
            LikeKind::Like
        } else if self.accept_kw("GLOB") {
            LikeKind::Glob
        } else {
            return Err(self.error("expected IN, BETWEEN, LIKE or GLOB"));
        };
        let pattern = Box::new(self.expr_bp(COMPARE)?);
        let escape = if self.accept_kw("ESCAPE") {
            Some(Box::new(self.expr_bp(COMPARE)?))
        } else {
            None
        };
        Ok(Expr::Like {
            kind,
            expr,
            pattern,
            escape,
            negated,
        })
    }
}

fn is_reserved(word: &str) -> bool {
    RESERVED.iter().any(|r| word.eq_ignore_ascii_case(r))
}

// Binding powers, loosest to tightest (SQLite's order of operators).
const OR: u8 = 1;
const AND: u8 = 2;
const EQUALITY: u8 = 4;
const COMPARE: u8 = 5;
const BITWISE: u8 = 6;
const ADD: u8 = 7;
const MULTIPLY: u8 = 8;
const CONCAT: u8 = 9;
const COLLATE: u8 = 10;
const PREFIX: u8 = 11;
