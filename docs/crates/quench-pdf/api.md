---
title: API reference
parent: quench-pdf
nav_order: 2
---

The crate root exports: `Document`, `Error`, `Result`, `extract_pages`, `extract_text`. The modules `cmap`, `doc`, `error`, `filter`, `font`, `glyphs`, `object`, `text` are private.

## Functions

```rust
pub fn extract_pages(data: &[u8]) -> Result<Vec<String>>
pub fn extract_text(data: &[u8]) -> Result<String>
```

* `extract_pages`: loads `Document`, walks the page tree and runs each page's content streams, returning one string per page in page order (fonts are cached across pages). Errors: `Error::Malformed` when the data is not a readable PDF, `Error::Unsupported` for encrypted ones. A stream that cannot be decoded gives an empty or shorter page text, not an error.
* `extract_text`: `extract_pages(data)?.join("\n\n")`.

For `tests/fixtures/cairo.pdf` (one page) `extract_text` has 784 bytes (checked while writing this page).

## `Error` and `Result`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Malformed(String),     // not a PDF, or damaged beyond reading
    Unsupported(String),   // valid PDF using something not implemented (encryption, ...)
}
impl fmt::Display for Error;   // "malformed PDF: {m}" / "unsupported PDF: {m}"
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
```

See [Errors](errors.md).

## `Document`

```rust
#[derive(Debug)]
pub struct Document { /* data, xref, trailer, cache: RefCell<..>, object_streams: RefCell<..> */ }

impl Document {
    pub fn load(data: &[u8]) -> Result<Self>;
    pub fn get_object(&self, num: u32) -> Result<Object>;
    #[must_use] pub fn resolve(&self, object: &Object) -> Object;
    #[must_use] pub fn get(&self, dict: &Dict, key: &str) -> Option<Object>;
    #[must_use] pub fn stream_data(&self, object: &Object) -> Option<Vec<u8>>;
    #[must_use] pub fn pages(&self) -> Vec<Page>;
    #[must_use] pub fn page_contents(&self, page: &Page) -> Vec<u8>;
}
```

* `load(data)`: looks for `%PDF-` in the first 4 096 bytes (bytes before it are dropped; the whole input is copied), reads the cross-reference data (`startxref` in the last 4 096 bytes, tables and streams, hybrid `XRefStm`, `/Prev` chain with cycle protection); if that fails or no catalog with `/Pages` is found, it rebuilds the table by scanning for `n g obj`. Errors: `Malformed("not a PDF: no %PDF- header")`, `Malformed("no document catalog")` and other `Malformed` details; `Unsupported("encrypted PDF")` when the trailer has `/Encrypt`.
* `get_object(num)`: the parsed object (cached); references inside are not followed; unknown numbers are `Object::Null`.
* `resolve(object)`: follows `Ref`s up to 32 hops; unresolvable or looping references become `Null`.
* `get(dict, key)`: `dict[key]` resolved; `None` when absent or null.
* `stream_data(object)`: the decoded bytes of a stream (or a reference to one) after all `/Filter`s; `None` when it is not a stream or a filter fails.
* `pages()`: leaf pages in tree order, with `Resources`, `MediaBox` and `Rotate` inherited from ancestors; tree depth capped at 64, each referenced node visited once.
* `page_contents(page)`: the page's `/Contents` stream(s), decoded and joined with `\n`.

**Unreachable types.** `Page` (`pub struct Page { pub dict: Dict }`), `Object`, `Dict` (`BTreeMap<String, Object>`), `Stream` and `Parser` are `pub` items of private modules and are *not* re-exported, so code outside the crate cannot name them. The methods above can be called and their results used through inference (`doc.pages().len()`, passing a `&Page` returned by `pages()` to `page_contents`), but a caller cannot write function signatures with these types. This looks unintended and is listed in the gap report.

## Object model (internal, for reference)

`Object` is `Null | Bool | Int(i64) | Real(f64) | Name(String) | String(Vec<u8>) | Array(Vec<Object>) | Dict(Dict) | Stream(Box<Stream>) | Ref(u32, u16) | Op(String)`, with accessors `as_int`, `as_f64`, `as_name`, `as_array`, `as_dict`. The parser nests at most 64 levels.
