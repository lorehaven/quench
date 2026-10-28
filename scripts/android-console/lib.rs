//! A console for the Android host's plugins: type a plugin, a method and JSON arguments, press Run
//! and read the answer. `scripts/android-e2e.fish` replaces a new project's `app/src/lib.rs` with
//! this file and drives it over the WebView's DevTools port.

use quench_serde::{Value, json};
use quench_ui::prelude::*;
use quench_ui_web::{bridge, spawn};

fn field(id: &'static str, placeholder: &'static str, signal: &Signal<String>) -> Element {
    let (shown, typed) = (signal.clone(), signal.clone());
    input()
        .attr("id", id)
        .attr("placeholder", placeholder)
        .attr_fn("value", move || shown.get())
        .on_input(move |v| typed.set(v.to_string()))
}

#[must_use]
pub fn app() -> View {
    let plugin = create_signal(String::new());
    let method = create_signal(String::new());
    let args = create_signal("{}".to_string());
    let out = create_signal("idle".to_string());
    let events = create_signal(Vec::<String>::new());
    let (run_plugin, run_method, run_args, run_out) =
        (plugin.clone(), method.clone(), args.clone(), out.clone());
    let feed = events.clone();
    // Events the host pushes (progress, intents) are listed as they arrive.
    bridge::on_event(move |plugin, name, data| {
        feed.update(|list| list.push(format!("{plugin}.{name} {}", json::to_string(&data))));
    });
    let shown_events = events;
    div()
        .child(h1().text("console"))
        .child(field("plugin", "plugin", &plugin))
        .child(field("method", "method", &method))
        .child(field("args", "{json}", &args))
        .child(
            button()
                .attr("id", "run")
                .attr("type", "button")
                .text("Run")
                .on_click(move || {
                    let (plugin, method, out) = (
                        run_plugin.get_untracked(),
                        run_method.get_untracked(),
                        run_out.clone(),
                    );
                    let args: Value = match json::from_str(&run_args.get_untracked()) {
                        Ok(v) => v,
                        Err(e) => {
                            out.set(format!("error: bad_args: {e}"));
                            return;
                        }
                    };
                    out.set("running".to_string());
                    spawn(async move {
                        out.set(match bridge::call::<Value, Value>(&plugin, &method, &args).await {
                            Ok(value) => format!("ok: {}", json::to_string(&value)),
                            Err(e) => format!("error: {}: {}", e.code, e.message),
                        });
                    });
                }),
        )
        .child(p().attr("id", "out").text_fn(move || out.get()))
        .child(ul().attr("id", "events").child(View::keyed(
            move || shown_events.get().into_iter().enumerate().collect::<Vec<_>>(),
            |(i, _)| *i,
            |(_, line)| li().text(line).into_view(),
        )))
        .into_view()
}

quench_ui_web::app!(app);
