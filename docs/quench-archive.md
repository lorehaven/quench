# Quench Archive

`quench-archive` (crate `quench_archive`) reads and writes tar and zip archives and the compression formats under them: deflate, gzip and zstd. It has no dependencies and replaces the `tar`, `zip`, `zstd` and `flate2` crates.

```rust
use quench_archive::{tar, zstd};

let mut builder = tar::Builder::new();
builder.append_file("rivet.toml", b"name = \"x\"")?;
let packed = zstd::compress(&builder.finish());

let bytes = zstd::decompress(&packed, 64 << 20)?;          // refuses to expand past 64 MiB
for entry in tar::Reader::new(&bytes) {
    let entry = entry?;                                     // path, kind, mode, uid, gid, mtime, link, data
}
```

Everything works on byte slices and `Vec<u8>`. Every decoder takes a limit on its output, so a small hostile input cannot expand without bound; `tar::Reader` hands out slices of the input, so it cannot allocate more than the archive itself.

## API

- `tar::Reader` (iterator of `Entry`) reads ustar, GNU long names and links, and pax extended headers (`path`, `linkpath`, `size`, `uid`, `gid`, `mtime`), plus base-256 numbers. Header checksums are verified. `tar::Builder` writes ustar, splitting long paths into prefix and name or emitting a GNU long-name entry; `Header::{file, directory, symlink}` give the metadata (mode 0644, owner root, mtime 0 by default, so archives are reproducible).
- `zip::Archive::new(reader)` takes any `Read + Seek`, reads the central directory (zip64 end records and extra fields included) and extracts single entries (`read`, `read_by_name`) with size and CRC-32 checks. Stored and deflated entries; encryption and other methods are `Error::Unsupported`. `zip::Writer` writes stored or deflated entries (no zip64 output).
- `deflate::{compress, decompress, inflate}`, `gzip::{compress, decompress}` (concatenated members, optional header fields).
- `zstd::{compress, decompress}`: the decoder handles every block and table type a dictionary-less stream can contain (Huffman and FSE tables sent, predefined or repeated), skippable and concatenated frames, and the content checksum; dictionaries are `Error::Unsupported`. The encoder writes single-segment frames (one per 64 MiB) of LZ77 sequences with predefined FSE tables and Huffman literals, with a content checksum. Output is deterministic.
- `checksum::{crc32, Crc32, adler32, xxh64}`.
- `Error`: `Truncated`, `Corrupt`, `Unsupported`, `Limit`, `Checksum`, `Io`.

## Compression

On 2 MB of Rust source the zstd encoder produces 513 KB, against 485 KB for `zstd -3`, 373 KB for `zstd -19` and 469 KB for `gzip -9`. Deflate output is within a percent of zlib level 6. Incompressible data grows by under 0.01%.

## Verification

- Deflate is checked in both directions against Python's zlib on random, repetitive, text-like and incompressible inputs at levels 0, 1, 6 and 9.
- The zstd decoder was run against `zstd` output at levels 1-22 (with and without checksums) over 120 generated inputs up to 1.5 MB; the encoder's output was decompressed by `zstd` for the same kind of inputs.
- tar fixtures come from GNU tar in ustar, gnu and pax formats; the writer's output is read back by GNU tar and Python's `tarfile`. zip fixtures come from Info-ZIP and Python (including a zip64 extra field); the writer's output passes `unzip -t` and `zipfile`.
