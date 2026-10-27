//! The interface. The server renders `app()` into the first response and the browser takes the
//! page over, building the same view.

use {{ snake }}_core::step;
use {{ snake }}_shared::greet;
use quench_ui::prelude::*;
use quench_ui_web::spawn;

/// A counter, and a greeting the server makes.
#[must_use]
pub fn app() -> View {
    let count = create_signal(0_i64);
    let (shown, up, down) = (count.clone(), count.clone(), count);
    let name = create_signal(String::new());
    let greeting = create_signal(String::new());
    let (typed, said, ask) = (name.clone(), greeting.clone(), name.clone());
    let hello = greeting;
    div()
        .child(h1().text("{{ name }}"))
        .child(
            p().child(span().attr("id", "count").text_fn(move || shown.get().to_string())),
        )
        .child(
            button()
                .attr("id", "up")
                .text("+")
                .on_click(move || up.update(|c| *c = step(*c, 1))),
        )
        .child(
            button()
                .attr("id", "down")
                .text("-")
                .on_click(move || down.update(|c| *c = step(*c, -1))),
        )
        .child(
            form()
                .on_submit(move || {
                    let (who, said) = (ask.get_untracked(), hello.clone());
                    // A call to a `#[server]` function reads like any other async function.
                    spawn(async move {
                        match greet(who).await {
                            Ok(g) => said.set(g.text),
                            Err(e) => said.set(e.message().to_string()),
                        }
                    });
                })
                .child(
                    input()
                        .attr("id", "name")
                        .attr("placeholder", "your name")
                        .attr_fn("value", move || name.get())
                        .on_input(move |v| typed.set(v.to_string())),
                )
                .child(button().attr("type", "submit").text("Greet")),
        )
        .child(p().attr("id", "greeting").text_fn(move || said.get()))
        .into_view()
}

quench_ui_web::app!(app);
