// Pixel arithmetic for a demo window: the sizes are small, and `mul_add` would hide the formula.
#![allow(
    clippy::cast_precision_loss,
    clippy::suboptimal_flops,
    clippy::unwrap_used
)]
//! A window for the tests: draws a frame, redraws on every click, and prints what it saw as lines.
//! `FIXTURE_SECONDS` (default 3) bounds how long it stays; closing the window ends it too.
use quench_desktop_wayland::{Window, WindowEvent};
use quench_raster::{Color, Path};
use quench_text::FontSet;
use std::sync::OnceLock;

const PARAGRAPH: &str = "Native text: Noto Sans, embedded, kerned and wrapped to the width of the window. Zażółć gęślą jaźń, Привет, мир! The quick brown fox jumps over the lazy dog; sphinx of black quartz, judge my vow. AV To Ty We Yo — pairs that kern. 0123456789 (parentheses) [brackets] {braces} \"quotes\" 'apostrophes' ~tilde @at #hash $dollar %percent &ampersand *star.";

fn fonts() -> &'static (FontSet, FontSet) {
    static FONTS: OnceLock<(FontSet, FontSet)> = OnceLock::new();
    FONTS.get_or_init(|| (FontSet::noto_sans(), FontSet::noto_sans_bold()))
}
use std::time::{Duration, Instant};

fn rss_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse().ok())
        })
        .unwrap_or(0)
}

fn draw(window: &mut Window, clicks: u32) -> std::io::Result<Duration> {
    let started = Instant::now();
    let (w, h) = window.size();
    {
        let mut canvas = window.begin_frame()?;
        canvas.clear(Color::from_hex("#1b1d23").unwrap());
        canvas.fill_rounded_rect(
            24.0,
            24.0,
            w as f32 - 48.0,
            64.0,
            12.0,
            Color::rgb(70, 130, 220),
        );
        canvas.stroke_rounded_rect(24.0, 24.0, w as f32 - 48.0, 64.0, 12.0, 2.0, Color::WHITE);
        for i in 0..clicks.min(40) {
            canvas.fill_rounded_rect(
                24.0 + i as f32 * 10.0,
                110.0,
                8.0,
                40.0,
                4.0,
                Color::rgb(255, 200, 0),
            );
        }
        // Text: a heading, and a paragraph wrapped to the window, drawn every frame (no glyph cache yet).
        let (regular, bold) = fonts();
        let title = bold.layout(
            "Native renderer, slice 1: text",
            28.0,
            Some(w as f32 - 48.0),
        );
        bold.draw(&mut canvas, &title, 24.0, 170.0, Color::WHITE);
        let body = regular.layout(PARAGRAPH, 18.0, Some(w as f32 - 48.0));
        regular.draw(
            &mut canvas,
            &body,
            24.0,
            170.0 + title.height + 12.0,
            Color::rgb(210, 214, 222),
        );
        let mut circle = Path::new();
        circle.ellipse(w as f32 / 2.0, h as f32 - 80.0, 50.0, 50.0);
        canvas.fill_path(&circle, 0.0, 0.0, Color::rgba(120, 220, 160, 220));
    }
    window.present()?;
    Ok(started.elapsed())
}

fn main() -> std::process::ExitCode {
    let seconds: u64 = std::env::var("FIXTURE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3);
    let app_id =
        std::env::var("FIXTURE_APP_ID").unwrap_or_else(|_| "dev.lorehaven.native-fixture".into());
    let mut window = match Window::open("Native fixture", &app_id, 480, 320) {
        Ok(window) => window,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                || error.kind() == std::io::ErrorKind::ConnectionRefused =>
        {
            eprintln!("no compositor: {error}");
            return std::process::ExitCode::from(77);
        }
        Err(error) => {
            eprintln!("error: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    println!("opened {:?}", window.size());
    let mut clicks = 0;
    let first = draw(&mut window, clicks).unwrap();
    println!("frame {first:?} rss {} KiB", rss_kib());
    // FIXTURE_BENCH=n: draw n more frames as fast as the compositor releases buffers, and report the time.
    if let Some(n) = std::env::var("FIXTURE_BENCH")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
    {
        let mut times = Vec::new();
        for i in 0..n {
            times.push(draw(&mut window, i).unwrap());
        }
        times.sort();
        let total: Duration = times.iter().sum();
        println!(
            "bench {n} frames: mean {:?} median {:?} max {:?} rss {} KiB",
            total / n.max(1),
            times[times.len() / 2],
            times[times.len() - 1],
            rss_kib()
        );
    }
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        let wait = deadline.saturating_duration_since(Instant::now());
        match window.next_event(Some(wait)) {
            Ok(Some(WindowEvent::CloseRequested)) => {
                println!("close");
                break;
            }
            Ok(Some(WindowEvent::Resized { width, height })) => {
                println!("resized {width}x{height}");
                let _ = draw(&mut window, clicks);
            }
            Ok(Some(WindowEvent::PointerButton {
                button,
                pressed: true,
                x,
                y,
            })) => {
                clicks += 1;
                let took = draw(&mut window, clicks).unwrap();
                println!(
                    "click {button} at {x:.0},{y:.0}: frame {took:?} rss {} KiB",
                    rss_kib()
                );
            }
            Ok(Some(other)) => println!("event {other:?}"),
            Ok(None) => {}
            Err(error) => {
                eprintln!("error: {error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    println!("done rss {} KiB", rss_kib());
    std::process::ExitCode::SUCCESS
}
