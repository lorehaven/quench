---
title: Working with files and formats
parent: Guides
nav_order: 12
---

How the format crates fit together: [quench-qr](../crates/quench-qr/index.md) makes an SVG, [quench-svg](../crates/quench-svg/index.md) turns it into pixels, [quench-image](../crates/quench-image/index.md) reads and writes PNG, [quench-archive](../crates/quench-archive/index.md) packs the results (tar, zip, gzip, zstd), and [quench-pdf](../crates/quench-pdf/index.md) reads text back out of PDFs. All of them are dependency-free apart from each other, work on byte slices, and bound their output so untrusted files cannot exhaust memory.

The examples were compiled and run against the crates while this guide was written; the printed numbers are what the code produced then.

## 1. Make a QR code and turn it into a PNG

```rust
let qr_svg: String = quench_qr::QrCode::new("https://example.com/invoice/42")?.to_svg(200);
let img = quench_svg::render(&qr_svg, 1.0)?;                          // quench_image::Image, RGBA
let png: Vec<u8> = quench_image::encode_png(img.width, img.height, &img.rgba);
assert_eq!(quench_image::decode(&png)?, img);                        // lossless round trip
# Ok::<(), Box<dyn std::error::Error>>(())
```

* `QrCode::to_svg(min_pixels)` returns a standalone SVG with a 4-module quiet zone and a whole number of pixels per module. `quench_svg::render(svg, scale)` fills the single `<path>` of runs with the non-zero rule and draws the white `<rect>` first.
* `encode_png` writes 8-bit RGBA, unfiltered, deflate-compressed: valid and fast, not small.
* `quench_image::decode` would *refuse* the SVG text itself: it recognises `<svg` or `<?xml` and answers `Error::Unsupported("SVG")` (display: `SVG images are not supported`). Rasterize with `quench-svg` first.

## 2. Bundle files into tar, gzip, zstd or zip

```rust
use quench_archive::{gzip, tar, zip, zstd, Error};
use std::io::Cursor;

let mut b = tar::Builder::new();
b.append_file("qr.svg", qr_svg.as_bytes())?;
b.append_file("qr.png", &png)?;
let tar_bytes = b.finish();                       // 8192 bytes for the two files above
let tgz = gzip::compress(&tar_bytes);             // 2808 bytes
let tzst = zstd::compress(&tar_bytes);            // 3043 bytes (the encoder uses fixed FSE tables)

// read it back, with a ceiling on the expanded size
let unpacked = gzip::decompress(&tgz, 64 << 20)?;
let names: Vec<String> = tar::Reader::new(&unpacked)
    .map(|entry| entry.map(|e| e.path))
    .collect::<Result<_, Error>>()?;
assert_eq!(names, ["qr.svg", "qr.png"]);

// zip: random access to single entries
let mut w = zip::Writer::new();
w.add("qr.svg", qr_svg.as_bytes())?;
w.add("qr.png", &png)?;
let mut archive = zip::Archive::new(Cursor::new(w.finish()?))?;
let png_again = archive.read_by_name("qr.png", 16 << 20)?;
assert_eq!(png_again, png);
// entries(): ("qr.svg", size 3337, compressed 860), ("qr.png", 2074, 1712) at the time of writing
# Ok::<(), Box<dyn std::error::Error>>(())
```

Choosing a container:

| Need | Use |
|---|---|
| a stream of files, reproducible bytes | `tar::Builder` (mtime 0, mode 0644 by default) |
| compress a tar | `gzip::compress` or `zstd::compress` on the tar bytes (both single-pass, in memory) |
| open one file out of many without reading the rest | `zip::Archive` over a `File` or `Cursor` |
| one compressed blob, no container | `deflate::compress` (raw), `gzip`, or `zstd` |

## 3. Never trust sizes

Every decoder takes a limit and refuses to go past it:

```rust
let bomb = quench_archive::zstd::compress(&vec![0u8; 5_000_000]);        // 169 bytes
let err = quench_archive::zstd::decompress(&bomb, 1 << 20).unwrap_err();
assert!(matches!(err, quench_archive::Error::Limit(_)));                  // "limit exceeded: more than 1048576 bytes"
```

* `zip::Archive::read` compares the entry's *declared* size to the limit before reading any data, then inflates with that size as the cap.
* `tar::Reader` never copies file data and never allocates for a declared size.
* `quench_image::decode` checks dimensions (16 384 per side, 64 Mi pixels) from the headers before allocating pixels; `quench_svg::render` applies the same limits to the requested output size.
* **Paths are your job.** Neither `tar::Reader` nor `zip::Archive` rejects `../`, absolute paths, drive letters or symlinks that escape the target directory. Validate `Entry::path` / `EntryInfo::name` before writing to disk.

## 4. Sniff and decode an unknown image

```rust
fn describe(bytes: &[u8]) -> String {
    match quench_image::decode(bytes) {
        Ok(img) => format!("{}x{} image", img.width, img.height),     // PNG, JPEG, GIF or WebP
        Err(quench_image::Error::Unsupported(what)) => format!("cannot decode: {what}"),
        Err(e) => format!("bad image: {e}"),
    }
}
```

`decode` picks the format from the first bytes (PNG signature, `FF D8`, `GIF8`, `RIFF....WEBP`). Animated GIF and WebP give their first frame. CMYK/arithmetic JPEG is `Unsupported`. See [Formats](../crates/quench-image/formats.md).

## 5. Pull the text out of a PDF

```rust
let pdf = std::fs::read("quench-pdf/tests/fixtures/cairo.pdf")?;
let text = quench_pdf::extract_text(&pdf)?;          // pages joined by a blank line
assert!(text.starts_with("Cairo text line 0: "));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Scans (images of text) give empty pages; encrypted files give `Error::Unsupported`. Use `extract_pages` for one string per page.

## 6. Putting it together: an export pipeline (sketch)

*Unverified:* this section is a design sketch, not compiled. A report endpoint that returns a zip of a PDF's text pages plus a QR code:

1. `quench_pdf::extract_pages(&bytes)?` for the text of each page.
2. `quench_qr::QrCode::new(url)?.to_svg(256)` for the code; keep the SVG, or rasterize it as in section 1 for a PNG.
3. `zip::Writer::add("page-1.txt", text.as_bytes())?` for each page and `add("qr.svg", ..)`.
4. `Response::new(StatusCode::OK).header("content-type", "application/zip").body(writer.finish()?)` with `quench-proto`'s `Response` ([quench-proto](../crates/quench-proto/api.md)).

Zip limits to keep in mind: 65 535 entries, 4 GiB total, no zip64 output.

## Where to look next

| Question | Page |
|---|---|
| exact tar/zip/zstd rules and limits | [quench-archive internals](../crates/quench-archive/internals.md) |
| which image features are supported | [quench-image formats](../crates/quench-image/formats.md) |
| which SVG elements are drawn | [quench-svg supported SVG](../crates/quench-svg/supported.md) |
| QR capacities and SVG layout | [quench-qr internals](../crates/quench-qr/internals.md) |
| PDF reading rules | [quench-pdf reading rules](../crates/quench-pdf/internals.md) |
