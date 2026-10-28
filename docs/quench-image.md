# Quench Image

`quench-image` (crate `quench_image`) decodes images to RGBA with no dependencies (it uses [`quench-archive`](./quench-archive.md)'s inflate and checksums), for the [native renderer](./quench-ui-native.md)'s `<img>`. Drawings (SVG) are [`quench-svg`](./quench-svg.md).

```rust
let image = quench_image::decode(&bytes)?;      // Image { width, height, rgba }
let png = quench_image::encode_png(image.width, image.height, &image.rgba);
```

**PNG**: every colour type and bit depth (1 to 16), palettes with `tRNS` transparency and colour keys, all five filters, **Adam7** interlacing, and a CRC check of every chunk.

**JPEG**: baseline and **progressive** Huffman files, 8 bits, grey or YCbCr (or RGB when an Adobe marker says so), any sampling (4:4:4, 4:2:2, 4:2:0, ...), restart intervals. The scans fill coefficient planes, which are dequantised and inverted with a floating-point IDCT; chroma that is subsampled is repeated, not smoothed, so edges differ a little from libjpeg's "fancy upsampling" (the tests allow a mean difference of 6 levels on subsampled files and 1 on full-resolution ones). Not read: arithmetic coding, lossless and 12-bit files, and four-channel (CMYK) files, which are named as unsupported.

**GIF**: the first frame, placed on the logical screen, with its palette (global or local), its transparent colour and interlacing; LZW is the variable-width kind GIF uses, and a file that ends early shows what arrived. Animation is not played.

**WebP**: the whole of it but animation. **Lossless** (VP8L): every transform (predictor, colour, subtract-green, colour indexing with pixel bundling), the colour cache, backward references, meta prefix codes. **Lossy** (VP8 key frames): the boolean entropy decoder, segments, quantisers, token partitions, the 16x16, 8x8 and 4x4 intra predictors, the inverse DCT and WHT, the **simple and the normal loop filter**, and YUV 4:2:0 converted with the reference fixed-point factors and **fancy (triangle) chroma upsampling**. The alpha plane (`ALPH`: raw or lossless-coded, with its horizontal, vertical and gradient filters) and the extended container (`VP8X`) are read; an **animated** file shows its first frame on the canvas, not played or blended. The constants of the format (`src/vp8_tables.rs`: the 10x10x9 sub-block mode probabilities, the coefficient probabilities and their update probabilities, the quantiser steps) are the numbers RFC 6386 defines. The lossy output is **bit-for-bit what libwebp makes** of the same files, for every option tested (`scripts/webp-fixtures.py` makes the files and their expected pixels through libwebp's encoder API: simple and strong filters, sharpness, eight token partitions, one or four segments, odd sizes, every alpha filter and compression).

Limits: 16384 pixels on a side and 64 million pixels. `encode_png` writes 8-bit RGBA (deflate-compressed, unfiltered), enough for tests and tools.

Errors are `Error::Unsupported(name)` (SVG is recognised by their signature and named, not decoded; the second is [`quench-svg`](./quench-svg.md)'s), `Error::Corrupt(why)` and `Error::TooLarge`. Damaged files are errors, never panics: the tests cut and flip bytes of real files everywhere.

The tests compare JPEGs and GIFs with the pixels another decoder (Pillow, libjpeg) made of the same files (`tests/data`).
