---
title: Internals
parent: quench-archive
nav_order: 4
---

Formats, limits, algorithm parameters and security properties, checked against the source.

## Output limits (decompression bombs)

* `deflate::{inflate, decompress}`, `gzip::decompress`, `zstd::decompress` take `limit: usize` (bytes of output). Every append to the output is checked against it *before* the data is produced (stored blocks, run-length blocks, literals, matches). Exceeding it is `Error::Limit`.
* `gzip::decompress` applies the limit to the total of all members (`limit - out.len()` for each).
* `zip::Archive::read` compares the entry's *declared* size with `limit` before reading, then inflates with the declared size as the limit, then requires the actual length to equal the declared size. A lying central directory therefore cannot produce more than it declares.
* `tar::Reader` never allocates for file content (slices of the input) and treats a declared size larger than the input as `Truncated`.
* Memory still used by the decoders: the whole output (`Vec<u8>`) plus, for zip, the compressed bytes of the single entry being read and the full central directory (bounded by the file length). zstd's window size field is ignored ("the whole output is kept in memory"): matches may reach back to the start of the current frame, never before it.

## tar

* Block size 512. Header fields (offsets): name 0..100, mode 100..108, uid 108..116, gid 116..124, size 124..136, mtime 136..148, checksum 148..156, type flag 156, link name 157..257, magic 257..263, version 263..265, prefix 345..500.
* Writer: `ustar\0` + `00`; GNU long-name entries use name `././@LongLink`, mode `0644`, magic `ustar  \0` (GNU style), type `L` or `K`, size = text length + 1, data = text + NUL, padded to 512.
* Reader accepts both `ustar` and old GNU magic; the `prefix` field is only honoured when bytes 257..263 are `ustar\0`.
* Not interpreted: device numbers, user and group names, atime/ctime, sparse maps. Hard links and symlinks are reported, not resolved.

## zip

Reading (little-endian throughout):

```text
End of central directory (EOCD), 22 bytes + comment:
  50 4B 05 06 | disk(2) | cd-disk(2) | entries-on-disk(2) | entries(2) | cd-size(4) | cd-offset(4) | comment-len(2)
Zip64 locator (20 bytes, immediately before EOCD):  50 4B 06 07 | ... | zip64-EOCD offset (8 at +8) | ...
Zip64 EOCD (56 bytes): 50 4B 06 06 | ... | entries(8 at +32) | cd-size(8 at +40) | cd-offset(8 at +48)
Central entry (46 bytes + name + extra + comment): 50 4B 01 02 | made-by(2) | needed(2) | flags(2 at +8) | method(2 at +10) | time(2) | date(2) | crc32(4 at +16) | csize(4 at +20) | size(4 at +24) | nlen(2 at +28) | elen(2 at +30) | clen(2 at +32) | disk(2) | iattr(2) | eattr(4) | local-offset(4 at +42)
Local header (30 bytes + name + extra): 50 4B 03 04 | ... | nlen(2 at +26) | elen(2 at +28)
```

* The EOCD is searched in the last 65 557 bytes. zip64 is used when the entry count is `0xFFFF` or the size/offset is `0xFFFFFFFF`; per entry, the zip64 extra field (`0x0001`) supplies, in order, size, compressed size and offset, for whichever of them is `0xFFFFFFFF`.
* Flag bit 0 = encrypted (refused on read), bit 11 = UTF-8 name.
* Writing: local headers with real CRC and sizes (no data descriptors), then the central directory, then EOCD. Offsets and sizes are 32-bit; no zip64 is ever written. Timestamps are fixed (DOS date `0x0021` = 1980-01-01, time 0).
* Security: names are not sanitised. Reject `..`, absolute paths and drive prefixes before extracting to disk ("zip slip").

## deflate

* Decoder: bit reader over the slice, canonical Huffman codes decoded a bit at a time (`Decoder`), over-subscribed and incomplete codes rejected, `distance reaches before the start` rejected. Window 32 KiB logically, but the whole output is the history.
* Encoder parameters: window 32 768, hash of 3 bytes into 15 bits (multiplier `0x9E3779B1`), chain depth 128, accept-at-length 128, max match 258, min match 3, 32 768 tokens per block. Code lengths come from package-merge (`huffman::code_lengths`), limited to 15 bits for literal/length and distance codes and 7 for the code-length code (deflate's limits). Each block is the smaller of stored, fixed and dynamic.

## gzip

Header written: `1F 8B 08 00 | MTIME=0 (4) | XFL=00 | OS=FF`. Trailer: CRC-32 and `ISIZE = len mod 2^32`, both little-endian.

## zstd

* Decoder: frame header (descriptor bits: content-size flag, single-segment, checksum flag, dictionary-id flag, reserved bit rejected), optional window descriptor (read and ignored), dictionary id (any non-zero value is `Unsupported`), content size (a mismatch with the actual output is `Corrupt("frame content size does not match")`), then blocks: raw (<= 128 KiB), RLE (<= 128 KiB), compressed (<= 128 KiB, reserved type 3 is `Corrupt`). Compressed blocks: literals (raw, RLE, Huffman compressed with one or four streams, treeless/repeat table reuse), sequences (predefined, RLE, FSE-compressed, repeat tables), repeat offsets, `BLOCK_MAX = 131072`. Content checksum: the low 32 bits of `xxh64(frame content, seed 0)`.
* Encoder: per 64 MiB chunk a frame with the single-segment bit, content-size field of 1, 2, 4 or 8 bytes (the 2-byte form stores `size - 256`) and the checksum flag; 128 KiB blocks; matcher with window 4 MiB (`1 << 22`), min match 4, 17-bit hash, chain depth 48, accept-at-length 96; literals Huffman-coded, sequences coded with the predefined FSE tables only. Output is valid for any zstd decoder and is deterministic. Compression ratio is below `zstd -3` (old page: 513 KB vs 485 KB for 2 MB of Rust source, not reproduced here).

## Checksums

CRC-32 table-driven (reflected `0xEDB88320`); Adler-32 (modulus 65521); xxHash64 (standard primes, seed parameter).

## Not provided

Streaming (`Read`/`Write`) interfaces for compression; zlib (RFC 1950) wrapper; bzip2, xz/lzma, brotli, 7z, rar; tar/zip path sanitising or extraction to the filesystem; encrypted zip; zstd dictionaries; compression levels (there is one fixed setting per format).
