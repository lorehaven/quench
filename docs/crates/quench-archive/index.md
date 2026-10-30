---
title: quench-archive
parent: Crates
nav_order: 3
has_children: true
permalink: /crates/quench-archive/
---

`quench-archive` (crate `quench_archive`) reads and writes tar and zip archives and the compression formats under them: raw deflate, gzip and zstd (decoder and encoder). It has **no dependencies** and replaces the `tar`, `zip`, `zstd` and `flate2` crates. It is also the inflate/CRC-32 provider of [quench-image](../quench-image/index.md) and [quench-pdf](../quench-pdf/index.md).

* Version: `0.1.2`. Edition 2024. No features. `unsafe_code = "deny"` through the workspace lints; none present.
* Everything works on byte slices and `Vec<u8>` (tar), or on `Read + Seek` (zip reading). Nothing touches the filesystem.
* **Every decoder takes a limit** on the size of its output, so a small hostile input cannot expand without bound. `tar::Reader` yields slices of its input and never allocates more than the archive itself.

## Install

```toml
[dependencies]
quench-archive = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Supported formats at a glance

| Format | Read | Write | Not supported |
|---|---|---|---|
| tar | ustar, GNU long names (`L`, `K`), pax (`x`; `g` skipped), base-256 numbers | ustar, GNU long-name entries for long paths and links | pax output, sparse files, multi-volume, compression (use gzip/zstd around it), path sanitisation |
| zip | stored and deflated entries, zip64 end record and extra fields, archive comment | stored or deflated (whichever is smaller), 32-bit sizes and offsets, at most 65 535 entries | encryption, other methods (bzip2, lzma, ...), multi-disk, zip64 output, data descriptors are not needed |
| deflate (RFC 1951) | stored, fixed and dynamic blocks | LZ77 with stored/fixed/dynamic blocks, whichever is smallest per block | zlib wrapper (RFC 1950) is not provided; `adler32` is available |
| gzip (RFC 1952) | all members of a concatenated file, optional header fields skipped, CRC-32 and length checked | one member, no name, mtime 0, OS 255 | header CRC-16 is skipped, not checked |
| zstd (RFC 8878) | all block types, Huffman and FSE tables (sent, predefined, repeated), skippable and concatenated frames, content checksum | single-segment frames of up to 64 MiB with content size and checksum | dictionaries, a frame's window size is not enforced |
| checksums | CRC-32, Adler-32, xxHash64 | | |

## Dependencies

None. Dev: `quench-test`.

## Where to go next

* [Usage](usage.md): worked examples.
* [API reference](api.md)
* [Errors](errors.md)
* [Internals](internals.md): formats, limits, parameters, security notes.
* [Testing](testing.md)
* The guide [Working with files and formats](../../guides/files-and-formats.md).

## Minimal example

```rust
use quench_archive::{tar, zstd};

let mut builder = tar::Builder::new();
builder.append_file("rivet.toml", b"name = \"x\"")?;
let packed = zstd::compress(&builder.finish());

let bytes = zstd::decompress(&packed, 64 << 20)?;       // refuses to expand past 64 MiB
for entry in tar::Reader::new(&bytes) {
    let entry = entry?;
    assert_eq!(entry.path, "rivet.toml");
}
# Ok::<(), quench_archive::Error>(())
```
