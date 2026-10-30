---
title: Reading rules
parent: quench-pdf
nav_order: 4
---

What the reader does and the limits it applies, from `doc.rs`, `object.rs`, `filter.rs`, `font.rs`, `cmap.rs`, `text.rs`.

## File structure

* Header `%PDF-` must appear in the first 4 096 bytes.
* Cross-reference: classic tables, xref streams, hybrid files (`/XRefStm`), `/Prev` chains (cycles stop the walk); the first entry seen for an object wins (newest section first). Object streams (`/Type /ObjStm`, `/N`, `/First`) are parsed once and cached.
* Recovery: if the xref data cannot be read or yields no catalog, every `n g obj` in the file is found by scanning and the last definition wins; `/Root` comes from the `trailer` dictionaries or xref-stream dictionaries found, or else from the first object whose `/Type` is `/Catalog`; object streams are expanded during the scan. Streams with a wrong `/Length` are located by `endstream`.
* Encryption: any trailer with `/Encrypt` is refused after load. There is no decryption.
* Limits: reference chains 32; page tree depth 64; object nesting 64; form XObject nesting 8; decoded stream size at most 512 MiB (`MAX_DECODED`, error `Malformed("FlateDecode: ...")` from the inflater when exceeded).

## Stream filters

`FlateDecode`/`Fl` (with or without the zlib header; leading whitespace skipped; the Adler-32 trailer is ignored), `LZWDecode`/`LZW` (honours `EarlyChange`), `ASCII85Decode`/`A85`, `ASCIIHexDecode`/`AHx`, `RunLengthDecode`/`RL`, plus PNG and TIFF predictors (`/Predictor`, `/Colors`, `/BitsPerComponent`, `/Columns`) after Flate and LZW. Any other filter name (`DCTDecode`, `JPXDecode`, `CCITTFaxDecode`, `JBIG2Decode`, `Crypt`) is `Error::Unsupported("stream filter NAME")` inside the decoder; callers of `stream_data` see `None`, and the page loses that content stream. Filters can be chained (array form) with `/DecodeParms` (`/DP`) matched by position.

## Fonts

* Simple fonts: base encoding `StandardEncoding` (default), `WinAnsiEncoding` (default for `TrueType`), `MacRomanEncoding`; `MacExpertEncoding` is mapped to Standard (approximation); `Encoding` dictionary with `BaseEncoding` and `Differences`. Glyph names resolve through the Adobe Glyph List (4 198 single-code-point entries in `glyphs.rs`), `uniXXXX` (groups of four hex digits), `uXXXX..uXXXXXX`, `a_b` ligature names; a suffix after `.` is ignored.
* `ToUnicode` CMaps (`bfchar`, `bfrange` incl. array form) win over encodings when they contain code to text entries.
* Composite fonts (`Type0`): code splitting by the embedded or predefined `Encoding` CMap's codespace ranges (two bytes at a time when there is none), `cidrange`, `ToUnicode`; widths from `/W` and `/DW`. Type 3 fonts: widths scaled by `FontMatrix`.
* Widths for advance: `/Widths` with `/FirstChar`, missing widths use the default width.
* Not done: reading glyph outlines or embedded font programs to guess text; fonts without a usable `ToUnicode` or encoding come out as the encoding table says (possibly wrong).

## Content streams

Operators interpreted: `q Q cm`, `BT ET`, `Tf`, `Tc Tw Tz TL Ts`, `Td TD Tm T*`, `Tj TJ ' "`, `Do` for form XObjects (own `Matrix` and `Resources`, depth < 8). Inline images (`BI ... EI`) are skipped. All other operators are ignored. Marked content, clipping, colours and images do not affect the text.

## Layout rules (reading order)

Text is emitted **in content-stream order**; positions only decide spacing and line breaks:

* a new line when the glyph is more than `0.6 x min(font sizes)` away from the previous glyph's baseline (or the direction of the text changes between horizontal and vertical);
* a space when the gap along the baseline exceeds `0.35 x (0.5 x max font size)` (about 0.175 em) and neither side is a space already;
* trailing spaces are trimmed at line ends; the page text is trimmed.
* Cleanup per character: ligatures U+FB00 to U+FB06 are expanded (`ff fi fl ffi ffl st st`), soft hyphen and NUL dropped, non-breaking space becomes a space, other control characters except tab dropped.

Consequences: multi-column pages and plot labels can come out in a different order than `pdftotext` gives; rotated text keeps its own line.
