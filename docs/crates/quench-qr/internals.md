---
title: Internals
parent: quench-qr
nav_order: 3
---

## Encoding pipeline

1. **Version selection.** The smallest version `v` in 1..=40 with `len <= capacity(v, level)`.
2. **Bit stream.** Mode indicator `0100` (byte), character count (8 bits for versions 1 to 9, 16 bits from version 10), the bytes, a terminator of up to 4 zero bits (fewer when the symbol is full), zero bits to a byte boundary, then pad codewords `0xEC, 0x11` alternating until the data capacity is filled.
3. **Blocks and error correction.** Per (version, level) tables `ECC_PER_BLOCK[level][version]` and `BLOCKS[level][version]` (ISO 18004) give the error-correction codewords per block and the block count. The data is cut into blocks (the first `blocks - total % blocks` blocks are the short ones, later blocks have one more data codeword); each block's Reed-Solomon remainder is computed over GF(256) with polynomial `x^8 + x^4 + x^3 + x^2 + 1` (0x11D), generator roots `a^0 .. a^(degree-1)`.
4. **Interleaving.** Data codewords column by column across blocks, then the error-correction codewords column by column.
5. **Function patterns.** Three 7x7 finder patterns with separators, timing patterns on row/column 6, alignment patterns (positions from the standard's formula; count `v / 7 + 2` per axis), the fixed dark module, and for version 7 and up two copies of the 18 version-information bits (BCH, generator `0x1F25`).
6. **Placement.** Zigzag from the lower right in pairs of columns, up then down, skipping column 6 and every function module.
7. **Masking.** All eight masks are tried: masks 0 to 7 are the standard formulas ((x+y) mod 2, y mod 2, x mod 3, (x+y) mod 3, (x/3 + y/2) mod 2, xy mod 2 + xy mod 3 = 0, (xy mod 2 + xy mod 3) mod 2, ((x+y) mod 2 + xy mod 3) mod 2). The penalty is rule 1 (runs of five or more: run - 2 points), rule 2 (2x2 blocks: 3 each), rule 3 (the 1:1:3:1:1 finder-like pattern with four light modules on either side: 40 each, symbol padded with light modules), rule 4 (deviation of the dark share from 50 %: 10 per started 5 % step beyond the first). The first lowest score wins (ties go to the lower mask number).
8. **Format information.** 15 bits: 2 level bits + 3 mask bits, BCH (generator `0x537`), XOR `0x5412`, written twice.

## Choice of mask and other encoders

The crate's tests compare the "Hello, World" level-M symbol with `qrencode -8` (same mask, identical modules). The old documentation stated that for other inputs mask choice differs from `qrencode` in a minority of penalty ties and that `zbarimg` decoded all samples; this was not re-run for this documentation (see testing).

## SVG output

`to_svg` produces exactly this shape (one line of XML header, one line of `svg`, trailing newline):

```text
<?xml version="1.0" standalone="yes"?>
<svg xmlns="http://www.w3.org/2000/svg" version="1.1" width="{pixels}" height="{pixels}" viewBox="0 0 {total} {total}" shape-rendering="crispEdges"><rect width="{total}" height="{total}" fill="#fff"/><path d="{path}" fill="#000"/></svg>
```

* `total = size + 8` (a quiet zone of 4 modules on each side; the standard asks for 4).
* `pixels = total * max(1, ceil(min_pixels / total))`.
* `path` is one subpath per horizontal run of dark modules, in row order: `M{x+4} {y+4}h{run}v1h-{run}z`. So the first finder row of "Hello, World" is `M4 4h7v1h-7z`.
* The result contains no scripts or external references, so it is safe to embed as a `data:image/svg+xml` URI.

## Limits

* Max payload: 2953 bytes at L, 2331 at M, 1663 at Q, 1273 at H (version 40, 177 x 177 modules).
* Encoding cost is linear in the size apart from the 8 mask evaluations (each O(size^2) with an 11-wide window scan).
* No input is ever interpreted; all byte values are valid.
