---
title: Testing
parent: quench-image
nav_order: 4
---

* Harness: `tests/unit.rs` (`[[test]] name = "unit"`, `harness = false`, `quench_test::main!()`), run with `cargo test -p quench-image`. Fixtures live in `tests/data/` (`*.jpg`, `*.gif`, `*.webp` and the expected pixels as `*.rgba`).
* PNG tests build files by hand (own chunk writer, row filters and Paeth): encoder/decoder round trip, every row filter undone, grey and palette images at every depth, colour keys, Adam7 pixel placement, damaged files (cut and flipped everywhere) are errors.
* JPEG and GIF: decoded pixels are compared with another decoder's output for the same files (baseline, progressive, subsampled, restart intervals; GIF palette, transparency, interlacing); the comparison allows a bounded difference for subsampled JPEGs; damaged files are errors.
* WebP: lossless fixtures (palette sizes 2, 4, 16, 200, alpha, noise, blocks) and lossy fixtures with alpha variants must match the expected pixel files; the first frame of an animation; damaged files are errors and never panic.
* The old page claims the lossy output is "bit-for-bit what libwebp makes" for the tested options, and mentions `scripts/webp-fixtures.py`; that script is outside this crate and was not run for this documentation.
