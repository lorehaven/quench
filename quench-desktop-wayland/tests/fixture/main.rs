//! A window for the tests: draws a frame and prints what it saw as lines. `FIXTURE_SECONDS` (default 1)
//! bounds how long it stays; closing the window ends it too. Exits 77 without a compositor.
#![allow(clippy::cast_precision_loss)] // window sizes are small
use quench_desktop_wayland::{Window, WindowEvent};
use quench_raster::{Color, Path};
use std::time::{Duration, Instant};

fn draw(window: &mut Window) -> std::io::Result<()> {
    let (w, h) = (window.size().0 as f32, window.size().1 as f32);
    {
        let mut canvas = window.begin_frame()?;
        canvas.clear(Color::rgb(27, 29, 35));
        canvas.fill_rounded_rect(24.0, 24.0, w - 48.0, 64.0, 12.0, Color::rgb(70, 130, 220));
        let mut circle = Path::new();
        circle.ellipse(w / 2.0, h / 2.0, 50.0, 50.0);
        canvas.fill_path(&circle, 0.0, 0.0, Color::rgba(120, 220, 160, 220));
    }
    window.present()
}

fn main() -> std::process::ExitCode {
    let seconds: u64 = std::env::var("FIXTURE_SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let mut window = match Window::open("Native fixture", "dev.lorehaven.native-fixture", 480, 320)
    {
        Ok(window) => window,
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
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
    if draw(&mut window).is_err() {
        return std::process::ExitCode::FAILURE;
    }
    println!("frame");
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        match window.next_event(Some(deadline.saturating_duration_since(Instant::now()))) {
            Ok(Some(WindowEvent::CloseRequested)) => break,
            Ok(Some(WindowEvent::Resized { .. })) => {
                if draw(&mut window).is_err() {
                    return std::process::ExitCode::FAILURE;
                }
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("error: {error}");
                return std::process::ExitCode::FAILURE;
            }
        }
    }
    println!("done");
    std::process::ExitCode::SUCCESS
}
