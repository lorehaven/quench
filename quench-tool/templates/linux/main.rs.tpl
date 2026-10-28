//! The Linux desktop host of {{ name }}: the bundle in a window, with the plugins in `quench_desktop`.

include!(concat!(env!("OUT_DIR"), "/bundle.rs"));

fn main() -> std::process::ExitCode {
    let mut app = quench_desktop::App::new("{{ title }}", bundle()).standard_plugins("{{ id }}");
    // The debug build is for trying things: the gallery's Native tab, and `quench dev --linux`.
    if cfg!(debug_assertions) {
        app = app.with_echo();
        if let Ok(url) = std::env::var("QUENCH_DEV_URL") {
            app = app.dev(&url);
        }
    }
    match quench_desktop::run(app) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{{ name }}: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
