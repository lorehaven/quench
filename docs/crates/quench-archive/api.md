---
title: API reference
parent: quench-archive
nav_order: 2
---

Public modules (from `lib.rs`): `checksum`, `deflate`, `error`, `gzip`, `tar`, `zip`, `zstd`; plus the crate-root re-exports `Error` and `Result`. The `huffman` module is private. All types are `Send + Sync`. Functions that decode are *total*: damaged input gives an `Error`, never a panic (except `zip::Archive::read` with a bad index).

## Crate root

```rust
pub use error::{Error, Result};
```

## `error`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Truncated,
    Corrupt(String),
    Unsupported(String),
    Limit(String),
    Checksum(String),
    Io(String),
}
impl fmt::Display for Error;               // texts in errors.md
impl std::error::Error for Error {}
impl From<std::io::Error> for Error;       // -> Error::Io(error.to_string())
pub type Result<T> = std::result::Result<T, Error>;
```

See [Errors](errors.md).

## `checksum`

```rust
#[derive(Debug, Clone, Copy)] pub struct Crc32(u32);
impl Crc32 {
    #[must_use] pub const fn new() -> Self;
    pub fn update(&mut self, data: &[u8]);
    #[must_use] pub const fn finish(self) -> u32;
}
impl Default for Crc32;    // = new()
#[must_use] pub fn crc32(data: &[u8]) -> u32;
#[must_use] pub fn adler32(data: &[u8]) -> u32;
#[must_use] pub fn xxh64(data: &[u8], seed: u64) -> u64;
```

* `crc32`: reflected CRC-32, polynomial `0xEDB88320` (gzip, zip, PNG); `crc32(b"123456789") == 0xCBF43926`. `Crc32` is the incremental form; `finish` does not consume state destructively (`self` is `Copy`).
* `adler32`: the zlib trailer checksum (not used by any codec in this crate; there is no zlib wrapper).
* `xxh64`: xxHash64 with `seed`; zstd's content checksum is its low 32 bits with seed 0.

## `deflate`

```rust
pub fn inflate(data: &[u8], limit: usize) -> Result<(Vec<u8>, usize)>;
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>>;
#[must_use] pub fn compress(data: &[u8]) -> Vec<u8>;
```

* `inflate`: decodes a raw RFC 1951 stream; returns the data and the number of **input bytes the stream occupied** (a partly read final byte counts as consumed). Anything after is the caller's. Errors: `Corrupt` (reserved block type, stored-block length check `LEN != !NLEN`, bad Huffman data, distance before start), `Truncated`, `Limit` (output would pass `limit`; message `more than {limit} bytes`). Empty input is `Truncated`.
* `decompress`: `inflate` without the length.
* `compress`: raw deflate. LZ77 with a 32 KiB window, hash chains (15-bit hash, chain depth 128, early cut-off at match length 128), lazy matching with one step of lookahead, max match 258, min 3; tokens are split into blocks of 32 768; each block is emitted as stored, fixed-Huffman or dynamic-Huffman, whichever is smallest. Deterministic.

## `gzip`

```rust
#[must_use] pub fn compress(data: &[u8]) -> Vec<u8>;
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>>;
```

* `compress`: one member: header `1F 8B 08 00 00000000 00 FF` (method deflate, no flags, MTIME 0, XFL 0, OS 255), the deflate stream, CRC-32 (LE), ISIZE (LE, `len mod 2^32`). No file name; output is reproducible.
* `decompress`: reads every member until the input is exhausted. Per member: magic `1F 8B`, method must be 8 (`Unsupported("gzip compression method N")`), reserved flag bits (`0xE0`) are `Corrupt("reserved gzip flags")`; `FEXTRA`, `FNAME`, `FCOMMENT` are skipped, `FHCRC` (2 bytes) is skipped without being verified; then CRC-32 and ISIZE are checked (`Checksum("gzip CRC-32")`, `Checksum("gzip length")`). `limit` applies to the total output across members. Bytes after the last member that do not form a member are an error; empty input is `Truncated`.

## `tar`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind { File, Directory, Symlink, HardLink, CharDevice, BlockDevice, Fifo, Other(u8) }
impl EntryKind {
    #[must_use] pub const fn is_file(self) -> bool;   // only EntryKind::File
    #[must_use] pub const fn is_dir(self) -> bool;    // only EntryKind::Directory
}
```

Type flag mapping when reading: NUL, `'0'`, `'7'` -> `File`; `'1'` HardLink; `'2'` Symlink; `'3'` CharDevice; `'4'` BlockDevice; `'5'` Directory; `'6'` Fifo; anything else -> `Other(flag)` (read as a file: its data follows the header). When writing, `File` uses `'0'`; `Other(f)` writes `f`. For Directory, Symlink, HardLink, CharDevice, BlockDevice and Fifo the header size is not a data length: no data is read or written.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry<'a> {
    pub path: String,
    pub kind: EntryKind,
    pub size: u64,
    pub mode: u32,        // permission bits, & 0o7777
    pub uid: u64,
    pub gid: u64,
    pub mtime: u64,       // seconds since the Unix epoch
    pub link: Option<String>,   // target of a symbolic or hard link
    pub data: &'a [u8],         // file content; empty for anything that is not a file
}
```

`size` is the size as declared in the header (or pax `size`). Directory paths always end in `/` (added when missing). `link` is `None` when the link field is empty.

```rust
#[derive(Debug, Clone)] pub struct Reader<'a> { /* data, position, done */ }
impl<'a> Reader<'a> { #[must_use] pub const fn new(data: &'a [u8]) -> Self; }
impl<'a> Iterator for Reader<'a> { type Item = Result<Entry<'a>>; }
```

* Iterates the entries of an in-memory archive. Ends at the first all-zero 512-byte block or when the data ends exactly at a block boundary. After an error the iterator yields that one `Err` and then `None`.
* A trailing partial block (data not ending on a 512 boundary) is `Error::Truncated`.
* Header checksum (offset 148, 8 bytes): the unsigned and the signed byte sums are both accepted; otherwise `Error::Checksum("tar header")`.
* Numbers: octal (leading spaces, NUL or space terminated; empty = 0) or GNU base-256 (top bit set); a negative base-256 number is `Unsupported("negative tar number")`; non-octal digit is `Corrupt("tar number is not octal")`; overflow `Corrupt("tar number overflows")`.
* Names: ustar `name` (100 bytes) with `prefix` (155 bytes at offset 345) joined by `/` only when the magic at 257 is `ustar\0`. GNU `L` (long name) and `K` (long link) entries and pax `x` records (`path`, `linkpath`, `size`, `uid`, `gid`, `mtime`; fractional parts of numbers are dropped; unknown keys ignored) override the next entry. Pax `g` global headers are skipped. All path strings must be valid UTF-8 (`Corrupt("a path is not valid UTF-8")`).
* A declared size larger than the remaining input is `Truncated` (nothing is allocated for it).
* **Paths are returned as stored.** No normalisation, no `..` or absolute-path rejection, no symlink-escape checks.

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub path: String, pub kind: EntryKind, pub mode: u32,
    pub uid: u64, pub gid: u64, pub mtime: u64, pub link: Option<String>,
}
impl Header {
    #[must_use] pub fn file(path: &str) -> Self;                     // mode 0o644, uid/gid 0, mtime 0, link None
    #[must_use] pub fn directory(path: &str) -> Self;                // mode 0o755
    #[must_use] pub fn symlink(path: &str, target: &str) -> Self;    // mode 0o777, link Some(target)
}

#[derive(Debug, Default)] pub struct Builder { /* out: Vec<u8> */ }
impl Builder {
    #[must_use] pub const fn new() -> Self;
    pub fn append_file(&mut self, path: &str, data: &[u8]) -> Result<()>;
    pub fn append(&mut self, header: &Header, data: &[u8]) -> Result<()>;
    #[must_use] pub fn finish(self) -> Vec<u8>;
}
```

* `append(header, data)`: `data` is written only when `header.kind` has data (File, Other). Errors: `Corrupt("a tar path cannot be empty or contain NUL")`. Directory paths get a trailing `/`. If the path does not fit the 100-byte `name`, the writer tries to split it at a `/` into `prefix` (at most 155 bytes) and `name` (at most 100 bytes); when that is impossible it emits a GNU `L` entry named `././@LongLink` first and truncates the header name to 100 bytes at a character boundary. A link target longer than 100 bytes gets a `K` entry likewise. Mode is stored as 7 octal digits (`mode & 0o7777`); uid, gid, size and mtime as octal when they fit the field (`< 8^(len-1)`), else GNU base-256. Header magic `ustar\0`, version `00`, checksum written as six octal digits, NUL, space (computed with the field read as spaces).
* `finish`: appends two 512-byte zero blocks.

## `zip`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method { Stored, Deflated, Other(u16) }       // 0, 8, any other number

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryInfo {
    pub name: String,
    pub method: Method,
    pub compressed_size: u64,
    pub size: u64,
    pub crc32: u32,
    /* offset: u64, encrypted: bool  (private) */
}
impl EntryInfo { #[must_use] pub fn is_dir(&self) -> bool; }   // name ends in '/'

#[derive(Debug)] pub struct Archive<R> { /* reader, entries */ }
impl<R: Read + Seek> Archive<R> {
    pub fn new(reader: R) -> Result<Self>;
    #[must_use] pub fn entries(&self) -> &[EntryInfo];
    #[must_use] pub fn index_of(&self, name: &str) -> Option<usize>;
    pub fn read_by_name(&mut self, name: &str, limit: u64) -> Result<Vec<u8>>;
    pub fn read(&mut self, index: usize, limit: u64) -> Result<Vec<u8>>;
}

pub struct Writer { /* out, written */ }     // Default; Debug prints "Writer(N entries)"
impl Writer {
    #[must_use] pub const fn new() -> Self;
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<()>;
    pub fn finish(self) -> Result<Vec<u8>>;
}
```

* `Archive::new`: reads the last 65 557 bytes (65 535 + 22) to find the end-of-central-directory record (searching backwards for `PK\x05\x06`), refuses multi-disk archives, follows the zip64 locator (20 bytes before the end record) and zip64 end record (56 bytes) when the 16-bit count or 32-bit size/offset fields are saturated, checks that the central directory lies inside the file, then parses every directory entry (signature `PK\x01\x02`; zip64 extra field `0x0001` supplies saturated size, compressed size and offset in that order). Names are UTF-8 when general-purpose flag bit 11 is set (invalid UTF-8 then is `Corrupt`), otherwise decoded lossily from the bytes (not as CP437). An entry whose offset is not inside the file is `Corrupt("entry starts outside the file")`. No entry is read yet.
* `entries()`: directory order. `index_of`: first entry with exactly that name (no normalisation).
* `read_by_name`: `Corrupt("no entry called {name}")` if missing, else `read`.
* `read(index, limit)`: **panics if `index >= entries().len()`**. Order of checks: encrypted flag -> `Unsupported("encrypted zip entries")`; declared `size > limit` -> `Limit("{name} is {size} bytes, more than {limit}")` (checked *before* reading data); local header signature `PK\x03\x04` (`Corrupt("local header signature")`); the local header's own name/extra lengths are skipped (the central directory's sizes are authoritative, so entries using data descriptors work); method Stored copies, Deflated inflates with the declared size as the output limit, other methods `Unsupported("zip compression method N")`; result length must equal `size` (`Checksum("{name} has the wrong size")`) and CRC-32 must match (`Checksum("{name} CRC-32")`).
* `Writer::add`: empty name -> `Corrupt("a zip entry needs a name")`; more than 65 535 entries or any 32-bit overflow -> `Limit`. A name ending in `/` is an empty directory entry. Deflates the data and keeps whichever of deflate/stored is smaller (method 8 / 0; "version needed" 20 / 10). Flag bit 11 (UTF-8) is set when the name is not ASCII. Timestamp is fixed at DOS date 1980-01-01 00:00 (`0x0021`, time 0), "version made by" 20, no extra fields, no comments. Duplicate names are not detected. `finish` writes the central directory and the 22-byte end record; `Limit("zip output passes 4 GiB")` past 4 GiB.

## `zstd`

```rust
pub fn decompress(data: &[u8], limit: usize) -> Result<Vec<u8>>;
#[must_use] pub fn compress(data: &[u8]) -> Vec<u8>;       // re-exported from the private `encode` module
```

* `decompress`: concatenated frames are one stream; skippable frames (magic `0x184D2A50` to `0x184D2A5F`) are skipped; any other magic is `Corrupt("not a zstd frame")`. Empty input is `Truncated`. See [Internals](internals.md#zstd) for what frames are accepted.
* `compress`: one frame per 64 MiB of input (a single empty frame for empty input), each a *single-segment* frame with content size and a content checksum. Deterministic.
