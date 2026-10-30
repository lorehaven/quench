---
title: Formats
parent: quench-image
nav_order: 3
---

What each decoder reads and rejects, checked against `src/lib.rs`, `gif.rs`, `jpeg.rs`, `webp.rs`, `vp8.rs`.

## Common rules

* Size limits: width and height at most 16 384, `width * height` at most `64 << 20`; zero dimensions are `Corrupt`. Exceeding the limits is `TooLarge` (JPEG, GIF, WebP, PNG).
* Output alpha is straight. Colour profiles, gamma, EXIF orientation and other metadata are ignored.

## PNG

* Chunk walk: every chunk's CRC-32 is verified (`Corrupt` otherwise); chunk lengths may not run past the end; stops at `IEND`. Needs `IHDR`, at least one `IDAT` and `IEND`.
* Colour type / depth pairs accepted: grey 1, 2, 4, 8, 16; RGB 8, 16; palette 1, 2, 4, 8; grey+alpha 8, 16; RGBA 8, 16. Compression and filter methods must be 0; interlace 0 or 1.
* `PLTE`: multiple of 3 bytes, at most 768. Palette indices beyond the table become black. `tRNS`: palette alpha per entry (missing entries are 255), or a grey/RGB colour key (16-bit compared before scaling); when present with colour types 4 and 6 it is ignored.
* Sample scaling: below 8 bits scaled with `v * 255 / (2^d - 1)`; 16 bits keep the high byte (no rounding).
* All five filters (none, sub, up, average, Paeth) and **Adam7** interlacing.
* zlib wrapper: first byte must have method 8 and the 16-bit header must be a multiple of 31; the trailing Adler-32 is **not verified** (the deflate stream is decompressed with the expected size as the output limit, and the trailer is ignored). Inflated size must equal the expected size exactly.
* Concatenated `IDAT` chunks are joined before inflating. Unknown chunks are skipped, including critical-looking ones.

## JPEG

* Markers handled: `DQT`, `DHT`, `DRI`, `SOF0` (baseline), `SOF1` (extended sequential), `SOF2` (progressive), `SOS`, `APP14` Adobe (transform byte), `EOI`; others are skipped.
* Requirements: 8-bit precision; 1 (grey) or 3 components, sampling factors 1 to 4 each; up to four quantisation and four DC/AC Huffman tables; restart intervals supported.
* Colour: one component -> grey; three components -> YCbCr to RGB with `R = Y + 1.402 Cr`, `G = Y - 0.344136 Cb - 0.714136 Cr`, `B = Y + 1.772 Cb`, unless the Adobe marker's transform byte is `0` (then RGB as is). Alpha is always 255.
* Pipeline: coefficient planes for all scans (progressive files are shown after the last scan), dequantisation, separable floating-point IDCT. Subsampled chroma is **repeated** (nearest), not interpolated, so pixels differ slightly from libjpeg's "fancy upsampling".
* Rejected as `Unsupported`: precision other than 8, four components (CMYK/YCCK), arithmetic coding and lossless/hierarchical SOF markers.
* Not applied: EXIF orientation, ICC profiles, thumbnails.

## GIF

* `GIF87a` and `GIF89a`. Only the **first image** is decoded, drawn on a logical screen of the file's width and height; the area outside the image rectangle stays fully transparent (0,0,0,0).
* Global or local colour table; the Graphic Control Extension's transparent index (if its flag bit is set) gives alpha 0 for those pixels; all other pixels alpha 255. Interlaced images are de-interlaced (4 passes). LZW with minimum code size 1 to 11; data that ends early is padded with index 0 ("what arrived is the picture").
* Not supported: animation (later frames ignored), disposal methods and frame delays, background colour (not painted), the loop extension (skipped as an extension block). Any block introducer other than `0x21` or `0x2C` before the first image is `Corrupt`.

## WebP

* Container: `RIFF`/`WEBP` with chunk walking (the RIFF size is clamped to the file length). `VP8X` extended header, `VP8L` lossless, `VP8 ` lossy, `ALPH` alpha for lossy, `ANMF` animation.
* **Lossless (VP8L)**: all four transforms (predictor, cross-colour, subtract-green, colour-indexing with pixel bundling), colour cache, backward references with the 120-entry distance map, meta prefix codes.
* **Lossy (VP8)**: key frames only (a non-key frame is `Unsupported`), boolean entropy decoder, segments, quantisers, multiple token partitions, 16x16/8x8/4x4 intra prediction, inverse DCT and WHT, simple and normal loop filters, YUV 4:2:0 to RGB with the reference fixed-point factors and triangle ("fancy") chroma upsampling. Dimensions are 14 bits; scaling bits are ignored.
* **Alpha** (`ALPH`): compression 0 (raw) or 1 (lossless-coded); filters none, horizontal, vertical, gradient. Applied to lossy images only.
* **Animation**: the first `ANMF` frame is placed at its offset on the `VP8X` canvas (the rest of the canvas is transparent black). No blending, disposal or timing.
* A `VP8X` canvas whose size differs from a still image's size is `Corrupt`.
