//! The syntax tree.

use crate::record::Value;

/// A bound parameter: `?`, `?3`, `:name`, `@name`, `$name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Param {
    /// 1-based position (a plain `?` takes the next one after the largest so far).
    Index(usize),
    Named(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    BitNot,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    /// `IS`: equality in which NULL equals NULL.
    Is,
    IsNot,
    Lt,
    Le,
    Gt,
    Ge,
    BitAnd,
    BitOr,
    ShiftLeft,
    ShiftRight,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Concat,
}

/// Which pattern match a `LIKE`-family operator is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LikeKind {
    Like,
    Glob,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    Param(Param),
    Column {
        table: Option<String>,
        name: String,
    },
    Unary(UnaryOp, Box<Self>),
    Binary(BinaryOp, Box<Self>, Box<Self>),
    IsNull {
        expr: Box<Self>,
        negated: bool,
    },
    Between {
        expr: Box<Self>,
        low: Box<Self>,
        high: Box<Self>,
        negated: bool,
    },
    InList {
        expr: Box<Self>,
        list: Vec<Self>,
        negated: bool,
    },
    InSelect {
        expr: Box<Self>,
        select: Box<Select>,
        negated: bool,
    },
    Like {
        kind: LikeKind,
        expr: Box<Self>,
        pattern: Box<Self>,
        escape: Option<Box<Self>>,
        negated: bool,
    },
    Case {
        operand: Option<Box<Self>>,
        whens: Vec<(Self, Self)>,
        otherwise: Option<Box<Self>>,
    },
    Function {
        name: String,
        args: Vec<Self>,
        distinct: bool,
        /// `count(*)`.
        star: bool,
    },
    Cast {
        expr: Box<Self>,
        type_name: String,
    },
    Collate {
        expr: Box<Self>,
        collation: String,
    },
    Subquery(Box<Select>),
    Exists(Box<Select>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResultColumn {
    /// `*`
    All,
    /// `t.*`
    TableAll(String),
    Expr {
        expr: Expr,
        alias: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    Left,
    Cross,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JoinConstraint {
    None,
    On(Expr),
    Using(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableSource {
    Table {
        name: String,
        alias: Option<String>,
    },
    Subquery {
        select: Box<Select>,
        alias: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Join {
    pub kind: JoinKind,
    pub source: TableSource,
    pub constraint: JoinConstraint,
}

#[derive(Debug, Clone, PartialEq)]
pub struct From {
    pub first: TableSource,
    pub joins: Vec<Join>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullsOrder {
    Default,
    First,
    Last,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OrderTerm {
    pub expr: Expr,
    pub descending: bool,
    pub nulls: NullsOrder,
}

/// One `SELECT ... FROM ... WHERE ... GROUP BY ... HAVING`.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectCore {
    pub distinct: bool,
    pub columns: Vec<ResultColumn>,
    pub from: Option<From>,
    pub where_clause: Option<Expr>,
    pub group_by: Vec<Expr>,
    pub having: Option<Expr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundOp {
    Union,
    UnionAll,
    Intersect,
    Except,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Select {
    pub core: SelectCore,
    pub compounds: Vec<(CompoundOp, SelectCore)>,
    pub order_by: Vec<OrderTerm>,
    pub limit: Option<Expr>,
    pub offset: Option<Expr>,
}

/// What to do when a row breaks a uniqueness constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnConflict {
    Abort,
    Replace,
    Ignore,
    Fail,
    Rollback,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpsertAction {
    Nothing,
    Update {
        sets: Vec<(String, Expr)>,
        where_clause: Option<Expr>,
    },
}

/// `ON CONFLICT (target) DO ...`
#[derive(Debug, Clone, PartialEq)]
pub struct Upsert {
    pub target: Vec<String>,
    pub action: UpsertAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InsertSource {
    Values(Vec<Vec<Expr>>),
    Select(Box<Select>),
    DefaultValues,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Insert {
    pub or: OnConflict,
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub source: InsertSource,
    pub upsert: Option<Upsert>,
    pub returning: Vec<ResultColumn>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub or: OnConflict,
    pub table: String,
    pub sets: Vec<(String, Expr)>,
    pub where_clause: Option<Expr>,
    pub returning: Vec<ResultColumn>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Delete {
    pub table: String,
    pub where_clause: Option<Expr>,
    pub returning: Vec<ResultColumn>,
}

/// A column of an index or a key: a name, and the direction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedColumn {
    pub name: String,
    pub descending: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ColumnConstraint {
    PrimaryKey {
        descending: bool,
        autoincrement: bool,
    },
    NotNull,
    Unique,
    Default(Expr),
    Check(Expr),
    Collate(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    /// The declared type as written, `None` when there is none.
    pub type_name: Option<String>,
    pub constraints: Vec<ColumnConstraint>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TableConstraint {
    PrimaryKey(Vec<IndexedColumn>),
    Unique(Vec<IndexedColumn>),
    Check(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTable {
    pub name: String,
    pub if_not_exists: bool,
    pub columns: Vec<ColumnDef>,
    pub constraints: Vec<TableConstraint>,
    pub without_rowid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateIndex {
    pub name: String,
    pub table: String,
    pub unique: bool,
    pub if_not_exists: bool,
    pub columns: Vec<IndexedColumn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionMode {
    Deferred,
    Immediate,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AlterAction {
    AddColumn(ColumnDef),
    RenameTable(String),
    RenameColumn { from: String, to: String },
    DropColumn(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    CreateTable(CreateTable),
    CreateIndex(CreateIndex),
    DropTable { name: String, if_exists: bool },
    DropIndex { name: String, if_exists: bool },
    AlterTable { table: String, action: AlterAction },
    Insert(Insert),
    Update(Update),
    Delete(Delete),
    Select(Select),
    Begin(TransactionMode),
    Commit,
    Rollback,
    Savepoint(String),
    Release(String),
    RollbackTo(String),
    Pragma { name: String, value: Option<Value> },
}
