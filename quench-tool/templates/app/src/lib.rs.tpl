//! The interface. The server renders `app()` into the first response and the browser takes the
//! page over, building the same view.

use {{ snake }}_core::step;
use quench_ui::prelude::*;

/// A counter: press a button, the number changes.
#[must_use]
pub fn app() -> View {
    let count = create_signal(0_i64);
    let (shown, up, down) = (count.clone(), count.clone(), count);
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
        .into_view()
}

quench_ui_web::app!(app);
