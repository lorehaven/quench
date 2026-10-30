---
title: Usage
parent: quench-archive
nav_order: 1
---

## 1. Create and read a tar archive

```rust
use quench_archive::tar::{Builder, EntryKind, Header, Reader};

let mut b = Builder::new();
b.append(&Header::directory("docs"), &[])?;
b.append_file("docs/readme.txt", b"hello")?;
b.append(&Header::symlink("latest", "docs/readme.txt"), &[])?;
let bytes = b.finish();           // ends with two zero blocks

let entries: Vec<_> = Reader::new(&bytes).collect::<Result<_, _>>()?;
assert_eq!(entries[0].path, "docs/");           // directories always end in '/'
assert_eq!(entries[1].data, b"hello");
assert_eq!(entries[2].kind, EntryKind::Symlink);
assert_eq!(entries[2].link.as_deref(), Some("docs/readme.txt"));
# Ok::<(), quench_archive::Error>(())
```

Pitfall: `Entry::path` is whatever the archive says (`../../etc/passwd` included). Reject absolute paths and `..` components yourself before writing to disk.

## 2. A `.tar.gz` / `.tar.zst` round trip

```rust
use quench_archive::{gzip, tar, zstd};

let mut b = tar::Builder::new();
b.append_file("a.txt", b"aaaa")?;
let tar_bytes = b.finish();

let tgz = gzip::compress(&tar_bytes);
let tzst = zstd::compress(&tar_bytes);

let limit = 256 << 20; // never accept more than 256 MiB of expanded data
assert_eq!(gzip::decompress(&tgz, limit)?, tar_bytes);
assert_eq!(zstd::decompress(&tzst, limit)?, tar_bytes);
# Ok::<(), quench_archive::Error>(())
```

## 3. Write a zip file and read one entry

```rust
use quench_archive::zip::{Archive, Writer};
use std::io::Cursor;

let mut w = Writer::new();
w.add("notes/", &[])?;                    // a name ending in '/' is a directory
w.add("notes/a.txt", b"hello hello hello")?;
let bytes = w.finish()?;

let mut zip = Archive::new(Cursor::new(bytes))?;      // reads only the central directory
let names: Vec<&str> = zip.entries().iter().map(|e| e.name.as_str()).collect();
assert_eq!(names, ["notes/", "notes/a.txt"]);
assert_eq!(zip.read_by_name("notes/a.txt", 1 << 20)?, b"hello hello hello");
# Ok::<(), quench_archive::Error>(())
```

`Archive::new` accepts a `std::fs::File` too. Pitfall: `Archive::read(index, limit)` panics for an out-of-range `index`.

## 4. Decompress hostile input safely

```rust
use quench_archive::{deflate, Error};

let bomb = deflate::compress(&vec![0u8; 10_000_000]);
assert!(bomb.len() < 20_000);
match deflate::decompress(&bomb, 1 << 20) {
    Err(Error::Limit(_)) => {}          // refused before expanding past 1 MiB
    other => panic!("{other:?}"),
}
```

Choose the limit from what the caller can afford, not from anything the stream declares.

## 5. Streams that carry trailing bytes

```rust
use quench_archive::deflate;

let mut framed = deflate::compress(b"payload");
framed.extend_from_slice(b"TRAILER");
let (data, used) = deflate::inflate(&framed, 1024)?;
assert_eq!(data, b"payload");
assert_eq!(&framed[used..], b"TRAILER");   // `used` counts the input bytes the stream occupied
# Ok::<(), quench_archive::Error>(())
```

This is how `gzip` finds its trailer and how PNG/PDF-style containers can be parsed.

## 6. Checksums

```rust
use quench_archive::checksum::{adler32, crc32, xxh64, Crc32};

assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
let mut c = Crc32::new();
c.update(b"1234");
c.update(b"56789");
assert_eq!(c.finish(), 0xCBF4_3926);
let _ = (adler32(b"abc"), xxh64(b"abc", 0));
```

(`0xCBF43926` is the standard CRC-32 check value of `"123456789"`.)

## 7. Tolerating concatenation and skippable frames

`gzip::decompress` and `zstd::decompress` read *all* members/frames and return their concatenated output; zstd skippable frames (magic `0x184D2A50..5F`) are ignored. Trailing bytes that are not a valid member/frame are an error (`Corrupt`/`Truncated`), not ignored.
