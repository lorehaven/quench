---
title: Errors
parent: quench-archive
nav_order: 3
---

One error type for the whole crate: `quench_archive::Error` (alias `Result<T>`). It derives `Debug, Clone, PartialEq, Eq`, implements `std::error::Error`, and converts from `std::io::Error` into `Io(String)`.

| Variant | Display | Meaning |
|---|---|---|
| `Truncated` | `unexpected end of data` | The input ends before the structure it describes. Also an empty gzip/zstd/deflate input, a tar size larger than the rest of the archive, a cut zip. |
| `Corrupt(String)` | `corrupt data: {m}` | Not valid for its format (bad signatures, bad lengths, bad Huffman tables, non-UTF-8 paths, non-octal tar numbers, ...). |
| `Unsupported(String)` | `unsupported: {m}` | Valid input using something not implemented: zstd dictionaries, zip encryption, zip methods other than stored/deflated, multi-disk zip, gzip method other than 8, negative tar numbers. |
| `Limit(String)` | `limit exceeded: {m}` | The output would pass the caller's `limit` (decoders), or a writer hit a format limit (zip 65 535 entries / 4 GiB). |
| `Checksum(String)` | `checksum mismatch: {m}` | A stored checksum does not match: tar header, gzip CRC-32 or length, zip CRC-32 or size, zstd content checksum. |
| `Io(String)` | `i/o error: {m}` | The reader behind a `zip::Archive` failed (the `io::Error` text). |

Matching on the variant is the supported way to react: `Limit` is an expected outcome for hostile or oversized input and does not mean the data is damaged.

## Which function returns what

| Function | Errors |
|---|---|
| `deflate::inflate` / `decompress` | `Corrupt`, `Truncated`, `Limit` |
| `gzip::decompress` | `Corrupt`, `Truncated`, `Checksum`, `Limit`, `Unsupported` |
| `zstd::decompress` | `Corrupt`, `Truncated`, `Checksum`, `Unsupported`, `Limit` |
| `tar::Reader` items | `Truncated`, `Corrupt`, `Checksum`, `Unsupported` |
| `tar::Builder::append*` | `Corrupt` (empty path or NUL) |
| `zip::Archive::new` | `Corrupt`, `Truncated`, `Unsupported`, `Io` |
| `zip::Archive::read*` | `Limit`, `Unsupported`, `Checksum`, `Corrupt`, `Truncated`, `Io` |
| `zip::Writer::add`, `finish` | `Corrupt`, `Limit` |
| `deflate::compress`, `gzip::compress`, `zstd::compress`, `checksum::*` | none |
