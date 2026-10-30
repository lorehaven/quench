---
title: API - system and async helpers
parent: quench-core
grand_parent: Crates
nav_order: 3
---

Reference for `bytes`, `map`, `registry`, `tmp`, `fs`, `ignore`, `dotenv`, `process`, `shell`, `future` and `stream`. Errors, encodings and time are in [API - errors, encodings, time, versions](api.md).

## `bytes`

An immutable, cheaply clonable byte buffer (`bytes::Bytes`' role). Clones and slices share one allocation (static or `Arc`-shared storage plus a start/end window).

```rust
pub struct Bytes { /* repr, start, end */ }   // Clone
```

| Method | Signature | Notes |
|---|---|---|
| `new` | `#[must_use] pub const fn new() -> Self` | Empty, no allocation. |
| `from_static` | `#[must_use] pub const fn from_static(bytes: &'static [u8]) -> Self` | No copy. |
| `copy_from_slice` | `#[must_use] pub fn copy_from_slice(data: &[u8]) -> Self` | |
| `len`, `is_empty` | `#[must_use] pub const fn len(&self) -> usize` / `is_empty(&self) -> bool` | |
| `slice` | `#[must_use] pub fn slice(&self, range: impl RangeBounds<usize>) -> Self` | Shares storage. Panics when out of bounds. |
| `split_to` | `#[must_use = "..."] pub fn split_to(&mut self, at: usize) -> Self` | Removes and returns the first `at` bytes. Panics when `at > len`. |
| `split_off` | `#[must_use = "..."] pub fn split_off(&mut self, at: usize) -> Self` | Removes and returns the bytes from `at`. Panics when `at > len`. |
| `to_vec` | `#[must_use] pub fn to_vec(&self) -> Vec<u8>` | |

Trait implementations: `Default`, `Deref<Target = [u8]>`, `AsRef<[u8]>`, `Borrow<[u8]>`, `From<Vec<u8>>`, `From<String>`, `From<&'static [u8]>`, `From<&'static str>`, `From<Bytes> for Vec<u8>`, `PartialEq` (with `Bytes`, `[u8]`, `&[u8]`, `Vec<u8>`, `str`, `&str`), `Eq`, `PartialOrd`, `Ord`, `Hash`, `FromIterator<u8>`, `IntoIterator for &Bytes` (items `&u8`), and `Debug` (`b"text\x00"` style: printable ASCII as is, the rest escaped).

## `map`

```rust
pub struct ShardedMap<K, V> { /* 16 x Mutex<HashMap<K, V>>, RandomState */ }
```

A concurrent map of 16 independently locked shards; access is by closure, never by guard, so a guard cannot be held across another call. A poisoned shard is ignored (data stays consistent because every operation is a single map call). Implements `Default` and `Debug`.

Methods on every `ShardedMap<K, V>`:

| Method | Signature |
|---|---|
| `new` | `#[must_use] pub fn new() -> Self` |
| `len` | `pub fn len(&self) -> usize` (snapshot) |
| `is_empty` | `pub fn is_empty(&self) -> bool` |
| `clear` | `pub fn clear(&self)` |
| `retain` | `pub fn retain(&self, keep: impl FnMut(&K, &mut V) -> bool)` (shards visited one at a time, not atomic overall) |
| `for_each` | `pub fn for_each(&self, visit: impl FnMut(&K, &V))` (same consistency) |

Methods when `K: Hash + Eq`:

| Method | Signature | Notes |
|---|---|---|
| `insert` | `pub fn insert(&self, key: K, value: V) -> Option<V>` | Returns the replaced value. |
| `remove` | `pub fn remove<Q>(&self, key: &Q) -> Option<V> where K: Borrow<Q>, Q: Hash + Eq + ?Sized` | |
| `contains_key` | `pub fn contains_key<Q>(&self, key: &Q) -> bool where K: Borrow<Q>, Q: Hash + Eq + ?Sized` | |
| `with` | `pub fn with<Q, R>(&self, key: &Q, read: impl FnOnce(&V) -> R) -> Option<R>` (same bounds) | The shard stays locked while `read` runs: keep it short, do not call back into the map. |
| `update` | `pub fn update<R>(&self, key: K, default: impl FnOnce() -> V, change: impl FnOnce(&mut V) -> R) -> R` | Atomic insert-if-absent then change (the entry API). |
| `update_existing` | `pub fn update_existing<Q, R>(&self, key: &Q, change: impl FnOnce(&mut V) -> R) -> Option<R>` | Only if present. |
| `remove_if` | `pub fn remove_if<Q>(&self, key: &Q, remove_if: impl FnOnce(&V) -> bool) -> bool` | Returns whether it removed. |

When also `V: Clone`: `pub fn get<Q>(&self, key: &Q) -> Option<V>` (a clone of the value).

```rust
use quench_core::map::ShardedMap;
let hits: ShardedMap<String, u32> = ShardedMap::new();
hits.update("home".to_string(), || 0, |n| *n += 1);
assert_eq!(hits.get("home"), Some(1));
```

## `registry`

Items registered before `main` by a link-time constructor (`inventory`'s role).

```rust
pub struct Registry<T: 'static> { /* items: Mutex<Vec<&'static T>> */ }   // Debug, Default
impl<T: 'static> Registry<T> {
    #[must_use] pub const fn new() -> Self
    pub fn register(&self, item: &'static T)
    #[must_use] pub fn iter(&self) -> std::vec::IntoIter<&'static T>   // snapshot, newest first
    #[must_use] pub fn len(&self) -> usize
    #[must_use] pub fn is_empty(&self) -> bool
}
impl<T: 'static> IntoIterator for &Registry<T> { type Item = &'static T; ... }
```

```rust
#[macro_export]
macro_rules! submit { ($registry:expr, $ty:ty, $value:expr) => { ... } }
```

`submit!(REGISTRY, Type, value)` declares a `static` of type `Type` initialised with `value`, plus an `extern "C"` constructor placed in the platform's init section (`.init_array` on Linux/Android/BSD/illumos/Solaris, `__DATA,__mod_init_func` on Apple targets, `.CRT$XCU` on Windows) that calls `$registry.register(&ITEM)`. See [internals](internals.md) for caveats.

```rust
use quench_core::registry::Registry;
static HANDLERS: Registry<Handler> = Registry::new();
quench_core::submit!(HANDLERS, Handler, Handler { name: "ping" });
```

## `tmp`

```rust
pub struct TempDir { /* path: PathBuf */ }   // Debug; AsRef<Path>; Drop
impl TempDir {
    pub fn new() -> io::Result<Self>
    pub fn new_in(parent: impl AsRef<Path>) -> io::Result<Self>
    #[must_use] pub fn path(&self) -> &Path
    #[must_use] pub fn into_path(self) -> PathBuf
}
```

`new` creates `<system temp dir>/quench-<16 hex digits>`; `new_in` does the same under `parent`. The directory is made with `create_dir` (not `create_dir_all`) so a name collision fails instead of sharing a directory. `Drop` removes it with its contents; `into_path` keeps it on disk.

## `fs`

Recursive directory walking (`walkdir`'s role).

```rust
pub struct Entry { /* path, file_type, depth */ }   // Debug, Clone
impl Entry {
    #[must_use] pub fn path(&self) -> &Path
    #[must_use] pub fn into_path(self) -> PathBuf
    #[must_use] pub fn file_name(&self) -> &std::ffi::OsStr
    #[must_use] pub const fn file_type(&self) -> FileType
    #[must_use] pub const fn depth(&self) -> usize
    pub fn metadata(&self) -> io::Result<Metadata>
}
pub struct Walk { /* root, follow_links, sort, max_depth, filter */ }   // Debug
impl Walk {
    #[must_use] pub fn new(root: impl AsRef<Path>) -> Self
    #[must_use] pub const fn follow_links(mut self, follow: bool) -> Self
    #[must_use] pub const fn sort_by_file_name(mut self) -> Self
    #[must_use] pub const fn max_depth(mut self, depth: usize) -> Self
    #[must_use] pub fn standard_filters(mut self) -> Self
    #[must_use] pub fn filter_entry(mut self, keep: impl FnMut(&Entry) -> bool + 'static) -> Self
}
impl IntoIterator for Walk { type Item = io::Result<Entry>; type IntoIter = WalkIter; }
pub struct WalkIter { /* ... */ }   // Debug; Iterator<Item = io::Result<Entry>>
```

Behaviour:

* The root is entry depth 0 and is yielded first; a directory is yielded before its contents. Without `sort_by_file_name` the order is the OS's (`read_dir`) order; with it, children are visited in file-name order. `max_depth(n)` does not descend below depth `n` (0 is the root alone). Defaults: no link following, unsorted, unlimited depth.
* `file_type()` reports a symlink as a link unless `follow_links(true)`; with it, a link to a directory is entered and a link that resolves to an ancestor yields an error entry (`file system loop found: <path> points to an ancestor`) instead of looping.
* A directory that cannot be opened is yielded as an entry, followed by the `io::Error` as the next item.
* `filter_entry`: entries for which `keep` is false are skipped and a skipped directory is not entered. `filter_entry` **replaces** any earlier filter, including the one installed by `standard_filters`, so call `filter_entry` first and `standard_filters` after it (the latter wraps the former).
* `standard_filters` skips hidden files/directories and what ignore files exclude (see [`ignore`](#ignore)); the root is never skipped. It wraps any earlier filter (both must accept).

```rust
use quench_core::fs::Walk;
for entry in Walk::new(".").sort_by_file_name().standard_filters() {
    let entry = entry?;
    println!("{}", entry.path().display());
}
```

## `ignore`

`.gitignore`-style rules (`ignore`'s role).

```rust
pub enum Match { None, Ignore, Whitelist }   // Debug, Clone, Copy, PartialEq, Eq
pub struct Gitignore { /* rules */ }          // Debug, Clone, Default
impl Gitignore {
    #[must_use] pub fn parse(text: &str) -> Self
    #[must_use] pub fn from_file(path: &Path) -> Self
    #[must_use] pub const fn is_empty(&self) -> bool
    #[must_use] pub fn matched(&self, relative: &str, is_dir: bool) -> Match
    #[must_use] pub fn matched_names(&self, relative: &str, is_dir: bool) -> Match
}
pub struct Ignorer { /* root, git_root, global, exclude, per-directory rules */ }   // Debug
impl Ignorer {
    #[must_use] pub fn new(root: &Path, hidden: bool, files: bool) -> Self
    pub fn is_ignored(&mut self, path: &Path, is_dir: bool) -> bool
}
```

* `Match::None`: no rule matches; `Ignore`: the last matching rule excludes the path; `Whitelist`: the last matching rule is a `!` rule.
* `Gitignore::parse`: one pattern per line, `#` comments, `!` negation, trailing `/` for directories only, `*`, `?`, `[a-z]` (with `!`/`^` negation and ranges) and `**` (a lone `**` segment matches any number of directories); `\` escapes; trailing unescaped spaces are dropped. `from_file` reads a file; a missing or unreadable file has no rules. `matched` takes a path below the file's directory (`/` or `\` separated). `matched_names` is for a file with no directory (the global ignore file): `relative` is the full path and a pattern containing `/` only matches if it starts with `**/`.
* `Ignorer::new(root, hidden, files)`: `hidden` skips dot files, `files` applies the ignore files. Layers: the global git ignore file (`$HOME/.config/git/ignore`), `.git/info/exclude`, then per directory from the repository root (or just the walk root outside a repository) down: `.gitignore` (only inside a repository) and `.ignore`. All `.ignore` files outrank all `.gitignore` files, deeper beats shallower, the last matching rule in a file wins, and an excluded directory is not entered. `is_ignored` is false for the root.

## `dotenv`

`.env` files (`dotenvy`'s role). Existing environment variables are never overridden.

```rust
pub struct ParseError { pub line: usize, pub message: String }   // Debug, Clone, PartialEq, Eq; line is 1-based
pub enum Error { Io(io::Error), Parse(ParseError) }              // Debug
pub fn parse(text: &str) -> Result<Vec<(String, String)>, ParseError>
pub fn load_from(path: impl AsRef<Path>) -> Result<(), Error>
#[must_use] pub fn find() -> Option<PathBuf>
pub fn load() -> Result<Option<PathBuf>, Error>
```

* Syntax: blank lines and `#` comments are skipped; an optional `export ` prefix; a value is bare (ends at a ` #` comment, trailing space trimmed; `\n` is a newline, `\ ` a space), `'single quoted'` (literal) or `"double quoted"` (escapes `\n \r \t \\ \" \$`); quoted values may span lines; adjacent pieces concatenate (`a"b"'c'`). `$NAME` and `${NAME}` expand in bare and double-quoted values from earlier lines of the file or the process environment (unset expands to nothing); a lone `$` stays.
* `parse` errors (`ParseError`): a line without `=`, an empty or invalid key, or an unterminated quote. Display: `.env line {line}: {message}`.
* `load_from` reads the file and sets each pair not already set (via `env::set`, see [internals](internals.md)). `Error::Io` when it cannot be read, `Error::Parse` when malformed (nothing is set then). `Error` displays `cannot read .env: {io error}` or the parse error, has `source()`, and `From<io::Error>` / `From<ParseError>`.
* `find`: the nearest `.env` in the current directory or one of its parents. `load`: `load_from` the nearest one; `Ok(None)` when there is none, else `Ok(Some(path))`.

## `process`

```rust
pub struct NotFound(pub String);    // Debug, Clone, PartialEq, Eq; Display, Error
pub fn which(name: impl AsRef<str>) -> Result<PathBuf, NotFound>
```

A name with a path separator is checked as given; a bare name is looked up in each `PATH` directory in order (on Windows also with each `PATHEXT` extension). On Unix an executable needs an execute bit; elsewhere any file. `NotFound` displays `cannot find binary path: {name}`.

## `shell`

```rust
#[must_use] pub fn split(input: &str) -> Option<Vec<String>>
#[must_use] pub fn quote(word: &str) -> Cow<'_, str>
#[must_use] pub fn join<I, S>(words: I) -> String where I: IntoIterator<Item = S>, S: AsRef<str>
```

`split` reads words as a POSIX shell: whitespace separates; `'..'` is literal; `".."` honours `\\`, `\"`, `\$`, `` \` `` and a line continuation; a backslash outside quotes escapes the next character; `#` at the start of a word starts a comment to end of line. `None` for an open quote or escape; `''` is one empty word. `quote` returns the word unchanged when it consists only of safe characters, else single-quoted. `join` produces a line that `split` reads back unchanged.

## `future`

```rust
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
pub type LocalBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
pub fn block_on<F: Future>(future: F) -> F::Output
pub fn join_all<I>(futures: I) -> JoinAll<I::Item> where I: IntoIterator, I::Item: Future
pub struct JoinAll<F: Future>   // Debug, Unpin, Future<Output = Vec<F::Output>>
pub fn catch_unwind<F: Future + UnwindSafe>(future: F) -> CatchUnwind<F>
pub struct CatchUnwind<F>       // Debug, Future<Output = Result<F::Output, Box<dyn Any + Send>>>
```

* `block_on`: runs a future to completion on the current thread, parking while it waits. For tests and tiny tools; real I/O belongs on tokio.
* `join_all`: runs all futures concurrently, resolves to the outputs in the order given.
* `catch_unwind`: a panic inside the future becomes `Err(payload)`. Wrap a non-`UnwindSafe` future in `std::panic::AssertUnwindSafe` first.

## `stream`

A minimal `Stream` and the adapters quench uses.

```rust
pub trait Stream {
    type Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>>;
    fn size_hint(&self) -> (usize, Option<usize>)   // provided
}
pub type BoxStream<'a, T> = Pin<Box<dyn Stream<Item = T> + Send + 'a>>;
```

`Stream` is implemented for `&mut S` and `Box<S>` (`S: Stream + Unpin + ?Sized`), for `Pin<P>` (`P: DerefMut + Unpin`, `P::Target: Stream`), and for the types below.

### `trait StreamExt: Stream` (blanket for every `Stream`)

| Method | Signature | Returns |
|---|---|---|
| `next` | `fn next(&mut self) -> Next<'_, Self> where Self: Unpin` | future of `Option<Item>`; `None` when finished |
| `map` | `fn map<T, F: FnMut(Self::Item) -> T>(self, f: F) -> Map<Self, F>` | transformed items |
| `chain` | `fn chain<U>(self, other: U) -> Chain<Self, U> where U: Stream<Item = Self::Item> + Unpin` | this stream then `other` |
| `filter_map` | `fn filter_map<T, F: FnMut(Self::Item) -> Option<T>>(self, f: F) -> FilterMap<Self, F>` | keeps `Some` results |
| `buffer_unordered` | `fn buffer_unordered(self, limit: usize) -> BufferUnordered<Self> where Self::Item: Future` | runs up to `limit` (minimum 1) item futures at once, yields outputs as they finish (not source order) |
| `collect_vec` | `fn collect_vec(self) -> CollectVec<Self>` | future of `Vec<Item>`; never returns for an endless stream |

All except `next` require `Self: Sized + Unpin`. Adapter types `Next`, `Map`, `Chain`, `FilterMap`, `CollectVec`, `BufferUnordered` are public and `Debug` (`BufferUnordered` has a manual `Debug`).

### `FuturesUnordered`

```rust
pub struct FuturesUnordered<Fut>   // Default, Debug, Stream<Item = Fut::Output>, FromIterator<Fut>
impl<Fut> FuturesUnordered<Fut> {
    #[must_use] pub const fn new() -> Self
    pub fn push(&mut self, future: Fut)
    #[must_use] pub const fn len(&self) -> usize
    #[must_use] pub const fn is_empty(&self) -> bool
}
```

Yields each output as soon as its future finishes. Every future is polled whenever the set is, which suits a handful of tasks. A pushed future first runs the next time the set is polled.

### Sources

```rust
pub fn iter<I: IntoIterator>(items: I) -> Iter<I::IntoIter>
pub fn once<T: Unpin>(item: T) -> Iter<std::option::IntoIter<T>>
pub fn empty<T: Unpin>() -> Iter<std::option::IntoIter<T>>
pub const fn unfold<S, T, F, Fut>(state: S, f: F) -> Unfold<S, F, Fut>
    where F: FnMut(S) -> Fut, Fut: Future<Output = Option<(T, S)>>
pub struct Iter<I>        // Debug, Clone, Stream (all items immediately ready)
pub struct Unfold<S, F, Fut>   // Stream
```

`unfold(state, f)`: `f(state)` yields the next item and the state for the following step, or `None` to finish. It turns a channel receiver into a stream:

```rust
use quench_core::stream::{unfold, StreamExt};
let mut s = unfold(rx, |mut rx| async move { rx.recv().await.map(|item| (item, rx)) });
while let Some(item) = s.next().await { /* ... */ }
```

*Unverified:* the `rx` above is a tokio `mpsc::Receiver` as in the source comment; the crate itself does not depend on tokio.
