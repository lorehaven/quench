//! The screen of {{ name }} on Android, drawn by this program instead of a WebView: the views are the same
//! `quench-ui` views, styled by the same stylesheets, laid out and painted by `quench-ui-native`. The Kotlin
//! host (`SurfaceActivity`) shows the frames and sends touches, keys and text. Standard output is the protocol's:
//! log to standard error.

use std::rc::Rc;

use quench_android_surface::{Bridge, Value};
use quench_style::theme::Theme;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

/// The app: replace this with your own views.
fn app(bridge: Bridge) -> View {
    View::lazy(move || {
        let count = create_signal(0_i64);
        let shown = count.clone();
        let agreed = create_signal(false);
        // A plugin of the Kotlin host (`EchoPlugin` here; see `plugins()` in Plugins.kt) answers with the device.
        let model = create_signal(String::from("host: not asked"));
        let shown_model = model.clone();
        let bridge = bridge.clone();
        div()
            .attr("id", "app")
            .child(h1().text("{{ name }}"))
            .child(p().attr("id", "count").text_fn(move || format!("pressed {} times", shown.get())))
            .child(button().attr("id", "press").attr("type", "button").text("Press").on_click(move || count.update(|c| *c += 1)))
            .child(input().attr("id", "name").attr("type", "text").attr("placeholder", "Your name"))
            .child(quench_ui::kit::checkbox("I agree", &agreed))
            .child(p().attr("id", "host").text_fn(move || shown_model.get()))
            .child(button().attr("id", "ask").attr("type", "button").text("Ask the host").on_click(move || {
                let model = model.clone();
                bridge.call("echo", "device", Value::Null, move |answer| {
                    model.set(match answer {
                        Ok(device) => format!("host: {}", device.get("model").and_then(Value::as_str).unwrap_or("?")),
                        Err(e) => format!("host failed: {e}"),
                    });
                });
            }))
            .into_view()
    })
}

fn main() -> std::io::Result<()> {
    let renderer = Rc::new(NativeRenderer::new());
    for (scheme, theme) in [("dark", Theme::DefaultDark), ("light", Theme::DefaultLight)] {
        let rules = Theme::theme(theme);
        renderer.add_stylesheet(&["@media (prefers-color-scheme: ", scheme, ") {\n", &rules, "\n}"].concat());
    }
    for sheet in [quench_style::tokens::stylesheet(), quench_style::mobile::stylesheet(), quench_style::components::stylesheet()] {
        renderer.add_stylesheet(&sheet);
    }
    renderer.add_stylesheet(
        "body { margin: 0; padding: calc(1rem + env(safe-area-inset-top)) calc(1rem + env(safe-area-inset-right)) calc(1rem + env(safe-area-inset-bottom)) calc(1rem + env(safe-area-inset-left)); font-family: system-ui, sans-serif; background: var(--bs-gray-950); color: var(--bs-gray-100) }\
         button, input { font: inherit; min-height: 48px; margin: .25rem 0; color: var(--bs-gray-100); background: var(--bs-gray-800); border: 2px solid var(--bs-gray-400); border-radius: .5rem; padding: 0 1rem }\
         input { display: block; box-sizing: border-box; width: 100% }",
    );
    // Held so the views stay mounted for as long as the program runs.
    let bridge = Bridge::new();
    let _mounted = mount(&renderer, &renderer.root(), app(bridge.clone()));
    quench_android_surface::run_with(&renderer, &bridge)
}
