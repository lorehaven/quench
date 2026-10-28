//! Draws the component gallery with the native stack and writes what a comparison with a browser needs:
//!
//! ```text
//! quench-example-native-gallery <out-dir> [width] [height]
//!   page.html    the document, with a `data-nk` key on every element, styles inline
//!   layout.json  the border box of every laid out element, by key
//!   native.png   the pixels
//! ```
//!
//! `scripts/native-gate.mjs` loads `page.html` in headless Chromium and compares.

// Window sizes and PNG lengths are small numbers.
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::fmt::Write as _;
use std::rc::Rc;

use quench_archive::{checksum, deflate};
use quench_example_ui_web_app::gallery;
use quench_layout::LayoutBox;
use quench_raster::Canvas;
use quench_style::theme::Theme;
use quench_ui::mount;
use quench_ui_native::{NativeRenderer, NodeId, NodeKind};

fn escape(text: &str, attribute: bool) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}

const VOID: &[&str] = &["input", "br", "img", "hr", "meta", "link"];

fn html(r: &NativeRenderer, node: NodeId, nk: usize, out: &mut String) {
    match r.kind(node) {
        NodeKind::Text(t) => out.push_str(&escape(&t, false)),
        NodeKind::Anchor => {}
        NodeKind::Element(tag) => {
            let _ = write!(out, "<{tag} data-nk=\"{nk}\"");
            for (name, value) in r.attrs(node) {
                let _ = write!(out, " {name}=\"{}\"", escape(&value, true));
            }
            let styles = r.styles(node);
            if !styles.is_empty() {
                let text: Vec<String> = styles.iter().map(|(p, v)| format!("{p}:{v}")).collect();
                let _ = write!(out, " style=\"{}\"", escape(&text.join(";"), true));
            }
            out.push('>');
            if !VOID.contains(&tag.as_str()) {
                for child in r.children(node) {
                    html(r, child, child.index(), out);
                }
                let _ = write!(out, "</{tag}>");
            }
        }
    }
}

fn rects(b: &LayoutBox, x: f32, y: f32, out: &mut String) {
    // Where the box is drawn: `translate()` moves a box and everything in it, as getBoundingClientRect() reports.
    let (tx, ty) = (
        b.style.transform.translate.0.resolve(b.width),
        b.style.transform.translate.1.resolve(b.height),
    );
    let (ax, ay) = (x + b.x + tx, y + b.y + ty);
    let _ = write!(
        out,
        "{{\"k\":{},\"x\":{ax:.2},\"y\":{ay:.2},\"w\":{:.2},\"h\":{:.2}}},",
        b.key, b.width, b.height
    );
    for c in &b.children {
        rects(c, ax, ay, out);
    }
}

fn png(canvas: &Canvas) -> Vec<u8> {
    let (w, h) = (canvas.width(), canvas.height());
    let mut raw = Vec::with_capacity((w * 4 + 1) * h);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            let p = canvas.pixel(x, y);
            raw.extend_from_slice(&[p.r, p.g, p.b, p.a]);
        }
    }
    let mut zlib = vec![0x78, 0x9c];
    zlib.extend(deflate::compress(&raw));
    zlib.extend(checksum::adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut chunk = |kind: &[u8; 4], data: &[u8]| {
        out.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend(data);
        out.extend(&body);
        out.extend(checksum::crc32(&body).to_be_bytes());
    };
    let mut header = Vec::new();
    header.extend((w as u32).to_be_bytes());
    header.extend((h as u32).to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    chunk(b"IHDR", &header);
    chunk(b"IDAT", &zlib);
    chunk(b"IEND", &[]);
    out
}

/// The gallery in a Wayland window, with pointer and keyboard: `quench-example-native-gallery --window`.
fn window() {
    let r = Rc::new(NativeRenderer::new());
    let dark = format!(
        "@media (prefers-color-scheme: dark) {{\n{}\n}}",
        Theme::theme(Theme::DefaultDark)
    );
    let light = format!(
        "@media (prefers-color-scheme: light) {{\n{}\n}}",
        Theme::theme(Theme::DefaultLight)
    );
    for sheet in [
        dark,
        light,
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
        gallery::GALLERY_STYLE.to_string(),
    ] {
        r.add_stylesheet(&sheet);
    }
    let _mounted = mount(&r, &r.root(), gallery::gallery());
    let mut host = match quench_desktop_native::Host::open(
        "Gallery",
        "dev.lorehaven.quench.gallery",
        (420, 760),
        &r,
    ) {
        Ok(host) => host,
        Err(e) => {
            eprintln!("the window failed: {e}");
            std::process::exit(1);
        }
    };
    // Say where the focus is whenever it moves, so a script can check the keyboard works.
    let mut last = None;
    while !host.closing() {
        if let Err(e) = host.step(std::time::Duration::from_millis(250)) {
            eprintln!("the window failed: {e}");
            std::process::exit(1);
        }
        let now = r.focused();
        if now != last {
            last = now;
            if let Some(f) = now {
                println!(
                    "focus: <{}> {:?}",
                    r.tag(f).unwrap_or_default(),
                    r.text_of(f)
                );
            } else {
                println!("focus: none");
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--window") {
        window();
        return;
    }
    let dir = args
        .get(1)
        .expect("usage: quench-example-native-gallery <out-dir> [width] [height]");
    let width: usize = args.get(2).and_then(|w| w.parse().ok()).unwrap_or(390);
    let height: usize = args.get(3).and_then(|h| h.parse().ok()).unwrap_or(844);
    std::fs::create_dir_all(dir).expect("create the output directory");

    let dark = format!(
        "@media (prefers-color-scheme: dark) {{\n{}\n}}",
        Theme::theme(Theme::DefaultDark)
    );
    let light = format!(
        "@media (prefers-color-scheme: light) {{\n{}\n}}",
        Theme::theme(Theme::DefaultLight)
    );
    let page = format!(
        "{}\n{}\n{}\n{}",
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
        gallery::GALLERY_STYLE
    );
    let r = Rc::new(NativeRenderer::new());
    for sheet in [&dark, &light, &page] {
        r.add_stylesheet(sheet);
    }
    r.set_viewport(width as f32, height as f32);
    let _mounted = mount(&r, &r.root(), gallery::gallery());

    let mut canvas = Canvas::new(width, height);
    let started = std::time::Instant::now();
    let boxes = r.render(&mut canvas);
    println!("rendered {width}x{height} in {:?}", started.elapsed());

    let mut body = String::new();
    html(&r, r.root(), r.root().index(), &mut body);
    let document = format!(
        "<!doctype html><html data-nk=\"0\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><style>{dark}\n{light}\n{page}</style></head>{body}</html>"
    );
    std::fs::write(format!("{dir}/page.html"), document).expect("write page.html");
    let mut json = String::from("[");
    rects(&boxes, 0.0, 0.0, &mut json);
    json.pop();
    json.push(']');
    std::fs::write(format!("{dir}/layout.json"), json).expect("write layout.json");
    std::fs::write(format!("{dir}/native.png"), png(&canvas)).expect("write native.png");
    println!("wrote {dir}/page.html, layout.json, native.png");
}
