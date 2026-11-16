//! PDF text extraction: real files from several producers, and hand-built files for the corners.

#![allow(clippy::cast_possible_truncation, clippy::format_push_string)]
// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(clippy::wildcard_imports)]

use quench_archive::checksum::adler32;
use quench_pdf::{Document, Error, extract_pages, extract_text};

fn pages(bytes: &[u8]) -> Vec<String> {
    extract_pages(bytes).expect("a readable PDF")
}

// ---------------------------------------------------------------- real files

#[quench_test::test]
fn reads_files_from_real_producers() {
    let reportlab = pages(include_bytes!("fixtures/rl_canvas.pdf"));
    assert_eq!(reportlab.len(), 2);
    assert!(
        reportlab[0].starts_with("Line 0: The quick brown fox jumps over the lazy dog."),
        "{}",
        reportlab[0]
    );
    assert_eq!(reportlab[0].lines().count(), 30);
    assert!(
        reportlab[1].contains("fn main() { println!(\"hello, world\"); } // 100% ok"),
        "{}",
        reportlab[1]
    );
    assert!(
        reportlab[1].contains("café naïve – “quoted” — dash £5 €7 ½"),
        "{}",
        reportlab[1]
    );

    // Object streams and a cross-reference stream.
    let objstm = pages(include_bytes!("fixtures/qpdf_objstm.pdf"));
    assert!(
        objstm[0].starts_with("Heading 0\nThe quick brown fox"),
        "{}",
        objstm[0]
    );
    assert!(objstm.len() >= 2);

    // TrueType subset fonts with ToUnicode maps.
    let cairo = pages(include_bytes!("fixtures/cairo.pdf"));
    assert_eq!(
        cairo[0].lines().next(),
        Some("Cairo text line 0: Zażółć gęślą jaźń ünï")
    );
    let office = pages(include_bytes!("fixtures/lo.pdf"));
    assert!(
        office[0].starts_with("Hello from LibreOffice."),
        "{}",
        office[0]
    );
    assert!(office[0].contains("ĄĘŚĆ polish chars"), "{}", office[0]);

    // Type 3 fonts.
    let plot = pages(include_bytes!("fixtures/mpl3.pdf"));
    assert!(plot[0].contains("annotation text here"), "{}", plot[0]);
    assert!(
        plot[0].contains("Quarterly results — Zażółć"),
        "{}",
        plot[0]
    );
    assert!(plot[0].contains("time (s)"), "{}", plot[0]);

    // LZW as libtiff writes it (variable width, growing past 9 bits), holding the content stream.
    let lzw = pages(include_bytes!("fixtures/lzw_libtiff.pdf"));
    assert_eq!(lzw[0].lines().count(), 80);
    assert!(
        lzw[0].starts_with("libtiff line 0 of text 0\nlibtiff line 1 of text 919"),
        "{}",
        lzw[0]
    );

    let fpdf = extract_text(include_bytes!("fixtures/fpdf.pdf")).unwrap();
    assert_eq!(fpdf.lines().count(), 20);
}

// ---------------------------------------------------------------- hand-built files

fn stream(extra: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< /Length {} {extra} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

/// A file whose object `n` (1-based) has body `objects[n - 1]`; object 1 is the catalog.
fn build(objects: &[Vec<u8>], trailer: &str) -> Vec<u8> {
    let mut out = b"%PDF-1.5\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R {trailer} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn font(extra: &str) -> Vec<u8> {
    format!("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica {extra} >>").into_bytes()
}

/// One page showing `content` with font F1 (object 5).
fn one_page(content: &[u8], font_object: Vec<u8>, extra_objects: &[Vec<u8>]) -> Vec<u8> {
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Fm1 7 0 R >> >> >>".to_vec(),
        stream("", content),
        font_object,
    ];
    objects.extend_from_slice(extra_objects);
    build(&objects, "")
}

fn text_of(content: &str, font_object: Vec<u8>, extra: &[Vec<u8>]) -> String {
    pages(&one_page(content.as_bytes(), font_object, extra)).remove(0)
}

#[quench_test::test]
fn shows_strings_with_spacing_and_line_breaks() {
    assert_eq!(
        text_of(
            "BT /F1 12 Tf 72 700 Td (Hello World) Tj 0 -14 Td (Second line) Tj ET",
            font(""),
            &[]
        ),
        "Hello World\nSecond line"
    );
    // Kerning adjustments stay inside a word; a large one is a space.
    assert_eq!(
        text_of(
            "BT /F1 12 Tf 72 700 Td [(Hel) -20 (lo) -400 (world)] TJ ET",
            font(""),
            &[]
        ),
        "Hello world"
    );
    // Leading and the quote operators.
    assert_eq!(
        text_of(
            "BT /F1 12 Tf 14 TL 72 700 Td (one) Tj T* (two) Tj (three) ' 2 1 (four) \" ET",
            font(""),
            &[]
        ),
        "one\ntwo\nthree\nfour"
    );
    // Tm, TD, Tc, Tw, Tz, Ts and q/Q/cm.
    assert_eq!(
        text_of(
            "q 1 0 0 1 10 20 cm BT /F1 10 Tf 1 0 0 1 0 100 Tm (a) Tj 0 -10 TD (b) Tj ET Q BT /F1 9 Tf 2 Tc 3 Tw 50 Tz 5 Ts 0 0 Td (c d) Tj ET",
            font(""),
            &[]
        ),
        "a\nb\nc d"
    );
    // Escapes and octal codes in literal strings, and hex strings.
    assert_eq!(
        text_of(
            "BT /F1 12 Tf 72 700 Td (\\(a\\) \\101\\102 \\\\ x\\\ny) Tj <4849> Tj ET",
            font(""),
            &[]
        ),
        "(a) AB \\ xyHI"
    );
    // Control characters and ligature code points are cleaned.
    assert_eq!(
        text_of(
            "BT /F1 12 Tf 72 700 Td <0102> Tj ET",
            font("/Encoding << /Differences [1 /fi /uniFB02] >>"),
            &[]
        ),
        "fifl"
    );
    // Text outside BT..ET state and unknown fonts are ignored.
    assert_eq!(
        text_of(
            "(stray) Tj BT /Nope 12 Tf (lost) Tj ET BT /F1 12 Tf (kept) Tj ET",
            font(""),
            &[]
        ),
        "kept"
    );
    // Rotated text stays on one line.
    assert_eq!(
        text_of("BT /F1 12 Tf 0 1 -1 0 100 100 Tm (up) Tj ET", font(""), &[]),
        "up"
    );
    // Operands pile up without an operator, and nested junk is tolerated.
    assert_eq!(
        text_of(
            "1 2 3 << /A [1 2 ] >> BT /F1 12 Tf (ok) Tj ET",
            font(""),
            &[]
        ),
        "ok"
    );
}

#[quench_test::test]
fn decodes_fonts() {
    // Differences name glyphs: Adobe names, `uniXXXX`, `uXXXX` and `a_b` ligatures.
    let diffs = "/Encoding << /BaseEncoding /WinAnsiEncoding /Differences [65 /eacute /uni0042 /u0043 /f_i /nonsense] >>";
    assert_eq!(
        text_of("BT /F1 12 Tf (ABCDE) Tj ET", font(diffs), &[]),
        "éBCfi"
    );
    assert_eq!(
        text_of(
            "BT /F1 12 Tf (\\200 \\222) Tj ET",
            font("/Encoding /WinAnsiEncoding"),
            &[]
        ),
        "€ ’"
    );
    assert_eq!(
        text_of(
            "BT /F1 12 Tf (\\200) Tj ET",
            font("/Encoding /MacRomanEncoding"),
            &[]
        ),
        "Ä"
    );
    assert_eq!(
        text_of("BT /F1 12 Tf (\\140\\47) Tj ET", font(""), &[]),
        "‘’"
    );

    // A ToUnicode map wins over the encoding.
    let cmap = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
        1 begincodespacerange <00> <FF> endcodespacerange\n\
        1 beginbfchar <01> <0058> endbfchar\n\
        2 beginbfrange <02> <04> <0041> <05> <06> [<0059> <005A00610062>] endbfrange\n\
        endcmap end end";
    let to_unicode = stream("", cmap);
    let f = font("/ToUnicode 6 0 R");
    assert_eq!(
        text_of("BT /F1 12 Tf <010203040506> Tj ET", f, &[to_unicode]),
        "XABCYZab"
    );

    // Widths decide where the next word starts.
    let widths = font(
        "/FirstChar 65 /LastChar 66 /Widths [500 1000] /FontDescriptor << /MissingWidth 100 >>",
    );
    assert_eq!(
        text_of(
            "BT /F1 10 Tf 0 0 Td (A) Tj 6.9 0 Td (B) Tj ET",
            widths.clone(),
            &[]
        ),
        "A B"
    );
    assert_eq!(
        text_of("BT /F1 10 Tf 0 0 Td (A) Tj 5.5 0 Td (B) Tj ET", widths, &[]),
        "AB"
    );

    // A composite font: two-byte codes, widths from /W and /DW, text from ToUnicode.
    let composite = b"<< /Type /Font /Subtype /Type0 /BaseFont /X /Encoding /Identity-H /DescendantFonts [8 0 R] /ToUnicode 6 0 R >>".to_vec();
    let descendant =
        b"<< /Type /Font /Subtype /CIDFontType2 /DW 500 /W [1 [600 700] 3 4 800] >>".to_vec();
    let cmap2 = stream("", b"1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfrange <0001> <0004> <0041> endbfrange");
    let content = "BT /F1 10 Tf 0 0 Td <00010002000300040005> Tj ET";
    let got = pages(&one_page(
        content.as_bytes(),
        composite,
        &[cmap2, b"<< >>".to_vec(), descendant],
    ))[0]
        .clone();
    assert!(got.starts_with("ABCD"), "{got}");

    // An embedded CMap that maps codes to CIDs through a different code space.
    let embedded = stream("", b"1 begincodespacerange <00> <FF> endcodespacerange 1 begincidrange <00> <FF> 0 endcidrange");
    let one_byte =
        b"<< /Type /Font /Subtype /Type0 /Encoding 6 0 R /DescendantFonts [8 0 R] >>".to_vec();
    let got = pages(&one_page(
        b"BT /F1 10 Tf <41> Tj ET",
        one_byte,
        &[embedded, b"<< >>".to_vec(), b"<< /DW 400 >>".to_vec()],
    ))[0]
        .clone();
    assert_eq!(got, "A");
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x9C];
    out.extend_from_slice(&quench_archive::deflate::compress(data));
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in data.chunks(4) {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        let mut value = u32::from_be_bytes(word);
        let mut digits = [0u8; 5];
        for d in digits.iter_mut().rev() {
            *d = (value % 85) as u8 + b'!';
            value /= 85;
        }
        out.extend_from_slice(&digits[..=chunk.len()]);
    }
    out.extend_from_slice(b"~>");
    out
}

/// A valid (if unambitious) LZW stream: a clear code, then each byte as a nine-bit literal.
fn lzw(data: &[u8]) -> Vec<u8> {
    let mut codes = vec![256u32];
    codes.extend(data.iter().map(|&b| u32::from(b)));
    codes.push(257);
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for code in codes {
        buffer = buffer << 9 | code;
        bits += 9;
        while bits >= 8 {
            out.push((buffer >> (bits - 8)) as u8);
            bits -= 8;
            buffer &= (1 << bits) - 1;
        }
    }
    if bits > 0 {
        out.push((buffer << (8 - bits)) as u8);
    }
    out
}

#[quench_test::test]
fn undoes_stream_filters() {
    let content = b"BT /F1 12 Tf 72 700 Td (filtered text) Tj ET";
    let hex: Vec<u8> = content
        .iter()
        .flat_map(|b| format!("{b:02X}").into_bytes())
        .chain(*b">")
        .collect();
    let mut run_length = vec![u8::try_from(content.len() - 1).unwrap()];
    run_length.extend_from_slice(content);
    run_length.extend_from_slice(&[255, b'!', 128]);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("/Filter /FlateDecode", zlib(content)),
        ("/Filter [/ASCIIHexDecode]", hex),
        ("/Filter /ASCII85Decode", ascii85(content)),
        ("/Filter /LZWDecode", lzw(content)),
        (
            "/Filter [/ASCII85Decode /FlateDecode]",
            ascii85(&zlib(content)),
        ),
        ("/Filter /RunLengthDecode", run_length),
    ];
    for (extra, data) in cases {
        let objects = vec![
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
            stream(extra, &data),
            font(""),
        ];
        let got = pages(&build(&objects, ""));
        let expected = "filtered text";
        assert_eq!(got[0], expected, "{extra}");
    }
    // PNG and TIFF predictors on the content (a one-column "image" of bytes: each row is one byte).
    let rows: Vec<u8> = {
        let mut out = Vec::new();
        let mut previous = 0u8;
        for &b in content {
            out.push(1); // Sub
            out.push(b.wrapping_sub(previous));
            previous = 0; // each row's left neighbour is outside the row
        }
        out
    };
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        stream(
            "/Filter /FlateDecode /DecodeParms << /Predictor 11 /Columns 1 >>",
            &zlib(&rows),
        ),
        font(""),
    ];
    assert_eq!(pages(&build(&objects, ""))[0], "filtered text");
    let tiff_rows: Vec<u8> = content.windows(1).map(|w| w[0]).collect();
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        stream(
            "/Filter /FlateDecode /DecodeParms << /Predictor 2 /Columns 1 >>",
            &zlib(&tiff_rows),
        ),
        font(""),
    ];
    assert_eq!(pages(&build(&objects, ""))[0], "filtered text");
}

#[quench_test::test]
fn form_xobjects_and_inline_images() {
    let form = stream(
        "/Type /XObject /Subtype /Form /BBox [0 0 100 100] /Matrix [1 0 0 1 0 0]",
        b"BT /F1 12 Tf (from the form) Tj ET",
    );
    let got = text_of(
        "BT /F1 12 Tf (before) Tj ET q 1 0 0 1 0 -30 cm /Fm1 Do Q",
        font(""),
        &[b"<< >>".to_vec(), form],
    );
    assert_eq!(got, "before\nfrom the form");
    // Image bytes may contain anything, including `EI` inside a word.
    let got = text_of(
        "q BI /W 2 /H 1 /BPC 8 /CS /G ID \x00EIX\x01 EI Q BT /F1 12 Tf (after) Tj ET",
        font(""),
        &[],
    );
    assert_eq!(got, "after");
    // A form that includes itself does not loop forever.
    let looping = stream(
        "/Subtype /Form /Resources << /Font << /F1 5 0 R >> /XObject << /Fm1 7 0 R >> >>",
        b"BT /F1 12 Tf (x) Tj ET /Fm1 Do",
    );
    let got = text_of("/Fm1 Do", font(""), &[b"<< >>".to_vec(), looping]);
    assert!(got.starts_with('x'));
}

#[quench_test::test]
fn page_trees_and_inheritance() {
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 3 /Resources << /Font << /F1 7 0 R >> >> >>"
            .to_vec(),
        b"<< /Type /Pages /Parent 2 0 R /Kids [5 0 R 6 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 8 0 R >>".to_vec(),
        b"<< /Type /Page /Parent 3 0 R /Contents [8 0 R 9 0 R] >>".to_vec(),
        b"<< /Type /Page /Parent 3 0 R >>".to_vec(),
        font(""),
        stream("", b"BT /F1 12 Tf (first) Tj ET"),
        stream("", b"BT /F1 12 Tf 0 -20 Td (second) Tj ET"),
    ];
    let got = pages(&build(&objects, ""));
    assert_eq!(got, ["first\nsecond", "", "first"]);
    assert_eq!(
        extract_text(&build(&objects, "")).unwrap(),
        "first\nsecond\n\n\n\nfirst"
    );
    let doc = Document::load(&build(&objects, "")).unwrap();
    assert_eq!(doc.pages().len(), 3);

    // A cycle in the page tree is cut.
    let cyclic = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [2 0 R 3 0 R] >>".to_vec(),
        b"<< /Type /Page >>".to_vec(),
    ];
    assert_eq!(pages(&build(&cyclic, "")).len(), 1);
}

#[quench_test::test]
fn damaged_files_are_rebuilt_or_refused() {
    let good = one_page(b"BT /F1 12 Tf (survivor) Tj ET", font(""), &[]);
    // A wrong startxref.
    let text = String::from_utf8_lossy(&good).into_owned();
    let at = text.rfind("startxref").unwrap();
    let wrong = [&good[..at], b"startxref\n7\n%%EOF\n"].concat();
    assert_eq!(pages(&wrong)[0], "survivor");
    // No cross-reference data at all, and junk before the header.
    let at = text.find("xref\n0 ").unwrap();
    let bare = [b"junk before\n".as_slice(), &good[..at], b"\n%%EOF\n"].concat();
    assert_eq!(pages(&bare)[0], "survivor");
    // A stream with a wrong /Length.
    let mut lying = good.clone();
    let needle = b"/Length 29";
    if let Some(i) = lying.windows(needle.len()).position(|w| w == needle) {
        lying[i + 9] = b'3';
    }
    assert_eq!(pages(&lying)[0], "survivor");

    assert!(matches!(
        extract_pages(b"not a pdf at all"),
        Err(Error::Malformed(_))
    ));
    assert!(matches!(
        extract_pages(b"%PDF-1.4\nnothing else"),
        Err(Error::Malformed(_))
    ));
    assert!(matches!(extract_pages(&[]), Err(Error::Malformed(_))));
    let encrypted = build(
        &[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [] >>".to_vec(),
        ],
        "/Encrypt << /Filter /Standard >>",
    );
    assert!(matches!(
        extract_pages(&encrypted),
        Err(Error::Unsupported(_))
    ));
    assert!(
        Error::Unsupported("x".into())
            .to_string()
            .contains("unsupported")
    );
    assert!(
        Error::Malformed("x".into())
            .to_string()
            .contains("malformed")
    );

    // Bad filters and truncated data never panic.
    for cut in (0..good.len()).step_by(11) {
        let _ = extract_pages(&good[..cut]);
    }
    let unsupported = one_page(b"BT /F1 12 Tf (x) Tj ET", font(""), &[]);
    let text = String::from_utf8_lossy(&unsupported)
        .replace("/Length 25 >>", "/Length 25 /Filter /JPXDecode >>");
    assert!(extract_pages(text.as_bytes()).is_ok());
}

#[quench_test::test]
fn objects_of_every_kind_parse() {
    // Names with escapes, nested containers, reals and odd numbers in a dictionary the reader must skip over.
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R /Odd << /A#20B [1 -2 +3 4. -.5 --6 true false null (s\\)) <48 49> /N#41me] /Empty >> >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        stream("", b"% a comment\nBT /F1 12 Tf (x) Tj ET"),
        font(""),
    ];
    assert_eq!(pages(&build(&objects, ""))[0], "x");
}

/// A textbook LZW encoder (variable width, early change), for streams that grow the code table.
fn lzw_full(data: &[u8]) -> Vec<u8> {
    let mut out_codes: Vec<(u32, u32)> = Vec::new(); // (code, width)
    let mut table: std::collections::HashMap<Vec<u8>, u32> = std::collections::HashMap::new();
    let mut next = 258u32;
    let width = |next: u32| match next {
        0..=511 => 9,
        512..=1023 => 10,
        1024..=2047 => 11,
        _ => 12,
    };
    out_codes.push((256, 9));
    let mut current: Vec<u8> = Vec::new();
    for &b in data {
        let mut candidate = current.clone();
        candidate.push(b);
        if candidate.len() == 1 || table.contains_key(&candidate) {
            current = candidate;
            continue;
        }
        let code = if current.len() == 1 {
            u32::from(current[0])
        } else {
            table[&current]
        };
        out_codes.push((code, width(next)));
        table.insert(candidate, next);
        next += 1;
        current = vec![b];
    }
    if !current.is_empty() {
        let code = if current.len() == 1 {
            u32::from(current[0])
        } else {
            table[&current]
        };
        out_codes.push((code, width(next)));
        next += 1;
    }
    out_codes.push((257, width(next)));
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u64, 0u32);
    for (code, w) in out_codes {
        buffer = buffer << w | u64::from(code);
        bits += w;
        while bits >= 8 {
            out.push((buffer >> (bits - 8)) as u8);
            bits -= 8;
            buffer &= (1 << bits) - 1;
        }
    }
    if bits > 0 {
        out.push((buffer << (8 - bits)) as u8);
    }
    out
}

fn png_encode(data: &[u8], columns: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut previous = vec![0u8; columns];
    for (row, line) in data.chunks(columns).enumerate() {
        let mut line = line.to_vec();
        line.resize(columns, 0);
        let kind = (row % 5) as u8;
        out.push(kind);
        for i in 0..columns {
            let left = if i > 0 { line[i - 1] } else { 0 };
            let up = previous[i];
            let up_left = if i > 0 { previous[i - 1] } else { 0 };
            let predicted = match kind {
                1 => left,
                2 => up,
                3 => u16::midpoint(u16::from(left), u16::from(up)) as u8,
                4 => {
                    let p = i32::from(left) + i32::from(up) - i32::from(up_left);
                    let (pa, pb, pc) = (
                        (p - i32::from(left)).abs(),
                        (p - i32::from(up)).abs(),
                        (p - i32::from(up_left)).abs(),
                    );
                    if pa <= pb && pa <= pc {
                        left
                    } else if pb <= pc {
                        up
                    } else {
                        up_left
                    }
                }
                _ => 0,
            };
            out.push(line[i].wrapping_sub(predicted));
        }
        previous = line;
    }
    out
}

fn content_with(extra: &str, data: &[u8]) -> Vec<u8> {
    build(
        &[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
            b"<< /Type /Page /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
            stream(extra, data),
            font(""),
        ],
        "",
    )
}

#[quench_test::test]
fn larger_streams_exercise_every_filter_path() {
    let mut content = String::new();
    for i in 0..60 {
        content.push_str(&format!(
            "BT /F1 12 Tf 72 {} Td (line number {i} of the page) Tj ET\n",
            780 - i * 12
        ));
    }
    let expected: Vec<String> = (0..60)
        .map(|i| format!("line number {i} of the page"))
        .collect();
    let expected = expected.join("\n");
    let bytes = content.as_bytes();

    assert_eq!(
        pages(&content_with("/Filter /LZWDecode", &lzw_full(bytes)))[0],
        expected
    );
    assert_eq!(
        pages(&content_with(
            "/Filter /LZWDecode /DecodeParms << /EarlyChange 1 >>",
            &lzw_full(bytes)
        ))[0],
        expected
    );
    let predicted = png_encode(bytes, 7);
    assert_eq!(
        pages(&content_with(
            "/Filter /FlateDecode /DecodeParms << /Predictor 15 /Columns 7 >>",
            &zlib(&predicted)
        ))[0],
        expected
    );
    // A trailing zero from the padding of the last row is only whitespace to the content parser.
    let mut tiff = bytes.to_vec();
    for row in tiff.chunks_mut(7) {
        for i in (1..row.len()).rev() {
            row[i] = row[i].wrapping_sub(row[i - 1]);
        }
    }
    assert_eq!(
        pages(&content_with(
            "/Filter /FlateDecode /DecodeParms << /Predictor 2 /Columns 7 >>",
            &zlib(&tiff)
        ))[0],
        expected
    );
    // Damaged encodings give an empty page rather than an error or a panic.
    for (extra, data) in [
        ("/Filter /ASCII85Decode", b"bad~byte!".to_vec()),
        ("/Filter /ASCII85Decode", b"<~z!!~>".to_vec()),
        ("/Filter /LZWDecode", vec![0xFF, 0xFF, 0xFF]),
        ("/Filter /FlateDecode", vec![1, 2, 3, 4]),
        ("/Filter /DCTDecode", vec![1, 2, 3]),
        (
            "/Filter /FlateDecode /DecodeParms << /Predictor 2 /Columns 2 /BitsPerComponent 4 >>",
            zlib(b"abc"),
        ),
    ] {
        assert_eq!(pages(&content_with(extra, &data))[0], "", "{extra}");
    }
}

#[quench_test::test]
fn a_damaged_cross_reference_stream_file_is_rebuilt_from_its_object_streams() {
    let good = include_bytes!("fixtures/qpdf_objstm.pdf");
    let at = good.windows(9).rposition(|w| w == b"startxref").unwrap();
    let wrong = [&good[..at], b"startxref\n9\n%%EOF\n"].concat();
    assert_eq!(pages(&wrong), pages(good));
}

quench_test::main!();

// ---------------------------------------------------------------- drawing a page

fn svg_of(content: &str, extra: &[Vec<u8>]) -> String {
    let bytes = one_page(content.as_bytes(), font(""), extra);
    Document::load(&bytes)
        .expect("a PDF")
        .render_page(0)
        .expect("a page")
        .svg
}

#[quench_test::test]
fn draws_paths_in_their_colours_under_a_flip_with_clips_and_dashes() {
    let svg = svg_of(
        "q 1 0 0 rg 10 10 100 50 re f Q q 0 0 1 RG 3 w [4 2] 0 d 20 20 m 80 90 l S Q q 0 0 50 50 re W n 0 1 0 rg 0 0 100 100 re f Q",
        &[],
    );
    assert!(svg.starts_with("<svg") && svg.ends_with("</svg>"));
    assert!(
        svg.contains("matrix(1 0 0 -1 0 792)"),
        "the page is flipped: {svg}"
    );
    assert!(svg.contains("fill=\"#ff0000\""), "{svg}");
    assert!(
        svg.contains("stroke=\"#0000ff\"")
            && svg.contains("stroke-width=\"3\"")
            && svg.contains("stroke-dasharray=\"4 2\""),
        "{svg}"
    );
    assert!(
        svg.contains("<clipPath id=\"c1\">")
            && svg.contains("<g clip-path=\"url(#c1)\">")
            && svg.contains("fill=\"#00ff00\""),
        "{svg}"
    );
    // The clip ends with the state it was set in.
    assert_eq!(
        svg.matches("<g ").count() - 1,
        svg.matches("</g>").count() - 1
    );
}

#[quench_test::test]
fn draws_text_with_each_glyph_where_the_file_puts_it() {
    let bytes = one_page(
        b"BT /F1 12 Tf 100 700 Td (Hi) Tj 0 -20 Td [(A) -500 (B)] TJ ET",
        font("/FirstChar 65 /LastChar 66 /Widths [600 700]"),
        &[],
    );
    let svg = Document::load(&bytes)
        .expect("a PDF")
        .render_page(0)
        .expect("a page")
        .svg;
    assert!(svg.contains("font-size=\"12\""), "{svg}");
    assert!(svg.contains("<tspan x=\"0 13.2\">AB"), "{svg}");
    assert!(svg.contains(">Hi</tspan>") || svg.contains(">Hi"), "{svg}");
    // The text is turned over to stand upright under the page's flip.
    assert!(svg.contains("matrix(1 0 0 -1 100 700)"), "{svg}");
    // A is 600 thousandths wide; the adjustment of 500 thousandths adds half the size.
    let line = svg.split("<text").nth(2).expect("a second text");
    let xs: Vec<f64> = line
        .split("<tspan x=\"")
        .nth(1)
        .and_then(|t| t.split('"').next())
        .map(|l| l.split(' ').filter_map(|v| v.parse().ok()).collect())
        .unwrap_or_default();
    assert_eq!(xs.len(), 2, "{line}");
    assert!((xs[1] - xs[0] - 13.2).abs() < 0.01, "{xs:?}");
    // Invisible text (a scan's text layer) is not drawn.
    let hidden = svg_of("BT /F1 12 Tf 3 Tr 100 700 Td (Hidden) Tj ET", &[]);
    assert!(!hidden.contains("Hidden"));
}

#[quench_test::test]
fn draws_every_page_of_real_files_and_reports_their_sizes() {
    for bytes in [
        &include_bytes!("fixtures/cairo.pdf")[..],
        include_bytes!("fixtures/mpl3.pdf"),
        include_bytes!("fixtures/lo.pdf"),
        include_bytes!("fixtures/rl_canvas.pdf"),
        include_bytes!("fixtures/fpdf.pdf"),
        include_bytes!("fixtures/qpdf_objstm.pdf"),
    ] {
        let doc = Document::load(bytes).expect("a PDF");
        assert_eq!(doc.page_count(), extract_pages(bytes).expect("text").len());
        for i in 0..doc.page_count() {
            let page = doc.render_page(i).expect("a page");
            assert!(page.width > 100.0 && page.height > 100.0);
            assert!(page.svg.starts_with("<svg") && page.svg.ends_with("</svg>"));
            assert_eq!(
                page.svg.matches("<g").count(),
                page.svg.matches("</g>").count()
            );
        }
        assert!(doc.render_page(doc.page_count()).is_none());
    }
    let lo = Document::load(include_bytes!("fixtures/lo.pdf")).expect("a PDF");
    assert!(lo.render_page(0).expect("a page").svg.contains("<tspan"));
}

#[quench_test::test]
fn gives_the_text_of_a_page_with_where_it_is_for_a_viewer_to_select() {
    let bytes = one_page(
        b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET BT /F1 10 Tf 0 1 -1 0 50 50 Tm (Up) Tj ET",
        font(""),
        &[],
    );
    let page = Document::load(&bytes)
        .expect("a PDF")
        .render_page(0)
        .expect("a page");
    // The upright text only; a page is 792 points tall, the baseline 700 up is 92 down.
    assert_eq!(page.runs.len(), 1, "{:?}", page.runs);
    let run = &page.runs[0];
    assert_eq!(run.text, "Hello");
    assert!(
        (run.x - 100.0).abs() < 0.01
            && (run.y - 92.0).abs() < 0.01
            && (run.size - 12.0).abs() < 0.01,
        "{run:?}"
    );
    // The test font has no widths, so Helvetica's are used: 2278 thousandths of 12.
    assert!((run.width - 27.336).abs() < 0.01, "{run:?}");
}

#[quench_test::test]
fn reads_the_outline_and_the_links_with_their_pages_and_addresses() {
    let objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R /Dests << /Two [4 0 R /Fit] >> >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Annots [5 0 R 6 0 R 11 0 R] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] >>".to_vec(),
        b"<< /Type /Annot /Subtype /Link /Rect [10 20 110 40] /A << /S /URI /URI (https://example.org/a) >> >>".to_vec(),
        b"<< /Type /Annot /Subtype /Link /Rect [10 100 60 120] /Dest [4 0 R /XYZ 0 300 0] >>".to_vec(),
        b"<< /Type /Outlines /First 8 0 R /Last 9 0 R /Count 3 >>".to_vec(),
        b"<< /Title (First) /Parent 7 0 R /Next 9 0 R /Dest [3 0 R /Fit] /First 10 0 R /Last 10 0 R >>".to_vec(),
        b"<< /Title <FEFF00530065006300EF> /Parent 7 0 R /Prev 8 0 R /A << /S /GoTo /D (Two) >> >>".to_vec(),
        b"<< /Title (Child) /Parent 8 0 R /Dest [4 0 R /Fit] >>".to_vec(),
        b"<< /Type /Annot /Subtype /Link /Rect [0 0 5 5] /A << /S /GoTo /D (Two) >> >>".to_vec(),
    ];
    let doc = Document::load(&build(&objects, "")).expect("a PDF");
    let outline = doc.outline();
    let shown: Vec<(&str, usize, Option<usize>)> = outline
        .iter()
        .map(|o| (o.title.as_str(), o.level, o.page))
        .collect();
    assert_eq!(
        shown,
        [
            ("First", 0, Some(0)),
            ("Child", 1, Some(1)),
            ("Sec\u{ef}", 0, Some(1))
        ]
    );
    let links = doc.render_page(0).expect("a page").links;
    assert_eq!(links.len(), 3, "{links:?}");
    assert_eq!(links[0].uri.as_deref(), Some("https://example.org/a"));
    // A box from the top: 300 high, the link's top edge at 40 from the bottom.
    assert!(
        (links[0].x - 10.0).abs() < 0.01
            && (links[0].y - 260.0).abs() < 0.01
            && (links[0].width - 100.0).abs() < 0.01
    );
    assert_eq!(links[1].page, Some(1));
    assert_eq!(links[2].page, Some(1), "a named destination");
    assert!(doc.render_page(1).expect("a page").links.is_empty());
}
