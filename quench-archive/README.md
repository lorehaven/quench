# quench-archive
Archives and compression with no dependencies: tar, zip, deflate, gzip and zstd (decoder and encoder).

## What it is
`quench-archive` reads and writes tar and zip archives and the compression formats under them: raw deflate, gzip and zstd. It has no dependencies and replaces the `tar`, `zip`, `zstd` and `flate2` crates. Everything works on byte slices and `Vec<u8>` (or `Read + Seek` for zip reading); nothing touches the filesystem.

Every decoder takes a limit on the size of its output, so a small hostile input cannot expand without bound. The crate is also the inflate and CRC-32 provider of `quench-image` and `quench-pdf`.

## Use cases
- Pack and unpack `.tar.gz` and `.tar.zst` bundles.
- Read entries from a zip file or write a new one.
- Decompress untrusted data with a hard output limit.
- Compute CRC-32, Adler-32 or xxHash64 checksums.

## Example
```rust
use quench_archive::{tar, zstd};

let mut builder = tar::Builder::new();
builder.append_file("rivet.toml", b"name = \"x\"")?;
let packed = zstd::compress(&builder.finish());

let bytes = zstd::decompress(&packed, 64 << 20)?;   // refuses to expand past 64 MiB
for entry in tar::Reader::new(&bytes) {
    let entry = entry?;
    assert_eq!(entry.path, "rivet.toml");
}
# Ok::<(), quench_archive::Error>(())
```

Reading one zip entry:

```rust
use quench_archive::zip::{Archive, Writer};
use std::io::Cursor;

let mut w = Writer::new();
w.add("notes/a.txt", b"hello hello hello")?;
let mut zip = Archive::new(Cursor::new(w.finish()?))?;
assert_eq!(zip.read_by_name("notes/a.txt", 1 << 20)?, b"hello hello hello");
# Ok::<(), quench_archive::Error>(())
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-archive/
