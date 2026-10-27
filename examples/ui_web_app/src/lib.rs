//! A quench-ui app for the browser. Build it with
//!
//! ```text
//! cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app
//! ```
//!
//! and serve it with `examples/ui_web_server`. `--features counter` or `--features todo` builds a
//! smaller app, which is how the sizes in the roadmap were measured.

pub mod gallery;
pub mod notes;
pub mod site;

use std::cell::Cell;
use std::rc::Rc;

use quench_serde::{Deserialize, Serialize};
use quench_ui::prelude::*;
use quench_ui_web::{Request, sleep, spawn};

/// A counter: the smallest useful app.
#[must_use]
pub fn counter() -> View {
    let count = create_signal(0);
    let (shown, up, down) = (count.clone(), count.clone(), count);
    div()
        .class("card")
        .child(h2().text("Counter"))
        .child(
            span()
                .attr("id", "count")
                .text_fn(move || shown.get().to_string()),
        )
        .child(
            button()
                .attr("id", "up")
                .text("+")
                .on_click(move || up.update(|c| *c += 1)),
        )
        .child(
            button()
                .attr("id", "down")
                .text("-")
                .on_click(move || down.update(|c| *c -= 1)),
        )
        .into_view()
}

#[derive(Clone)]
struct Todo {
    id: u32,
    title: String,
    done: Signal<bool>,
}

/// A todo list: input, keyed list, per-item state.
#[must_use]
pub fn todo() -> View {
    let todos = create_signal(Vec::<Todo>::new());
    let draft = create_signal(String::new());
    let next_id = Rc::new(Cell::new(1_u32));

    let add = {
        let (todos, draft, next_id) = (todos.clone(), draft.clone(), Rc::clone(&next_id));
        move || {
            let title = draft.get_untracked();
            if title.trim().is_empty() {
                return;
            }
            let id = next_id.get();
            next_id.set(id + 1);
            todos.update(|t| {
                t.push(Todo {
                    id,
                    title,
                    done: create_signal(false),
                });
            });
            draft.set(String::new());
        }
    };
    let (list, left, remove_from) = (todos.clone(), todos.clone(), todos);
    let (draft_value, draft_input) = (draft.clone(), draft);
    let add_on_enter = add.clone();

    div()
        .class("card")
        .child(h2().text("Todos"))
        .child(
            form()
                .attr("id", "add-form")
                .on_submit(add)
                .child(
                    input()
                        .attr("id", "draft")
                        .attr("placeholder", "what needs doing?")
                        .attr_fn("value", move || draft_value.get())
                        .on_input(move |v| draft_input.set(v.to_string()))
                        .on_keydown(move |k| {
                            if k.key == "Enter" {
                                add_on_enter();
                            }
                        }),
                )
                .child(button().attr("type", "submit").text("Add")),
        )
        .child(p().attr("id", "left").text_fn(move || {
            format!(
                "{} left",
                left.with(|t| t.iter().filter(|x| !x.done.get()).count())
            )
        }))
        .child(ul().child(View::keyed(
            move || list.get(),
            |t| t.id,
            move |t| {
                let (done_class, done_toggle) = (t.done.clone(), t.done.clone());
                let (todos, id) = (remove_from.clone(), t.id);
                li().class_if("done", move || done_class.get())
                    .child(span().text(t.title))
                    .child(button().class("toggle").text("toggle").on_click(move || {
                        done_toggle.update(|d| *d = !*d);
                    }))
                    .child(button().class("remove").text("x").on_click(move || {
                        todos.update(|l| l.retain(|x| x.id != id));
                    }))
                    .into_view()
            },
        )))
        .into_view()
}

/// A todo the server knows about.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ServerTodo {
    pub id: u32,
    pub title: String,
    pub done: bool,
}

/// What the server renders the page from and embeds in it, so the browser starts from the same
/// data without asking for it again.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct AppState {
    pub server_todos: Vec<ServerTodo>,
}

impl AppState {
    /// The JSON the page embeds.
    #[must_use]
    pub fn to_json(&self) -> String {
        quench_serde::json::to_string(self)
    }
}

/// Todos from the server: given in `initial` when the server rendered them, fetched from
/// `/api/todos` otherwise.
#[must_use]
pub fn server_todos(initial: Option<Vec<ServerTodo>>) -> View {
    let status = create_signal("loading...".to_string());
    let items = create_signal(Vec::<ServerTodo>::new());
    let load = {
        let (status, items) = (status.clone(), items.clone());
        move || {
            status.set("loading...".to_string());
            let (status, items) = (status.clone(), items.clone());
            spawn(async move {
                match Request::get("/api/todos")
                    .header("accept", "application/json")
                    .send()
                    .await
                {
                    Ok(r) if r.ok() => {
                        match quench_serde::json::from_str::<Vec<ServerTodo>>(&r.text()) {
                            Ok(list) => {
                                status.set(format!("{} from the server", list.len()));
                                items.set(list);
                            }
                            Err(e) => status.set(format!("bad response: {e}")),
                        }
                    }
                    Ok(r) => status.set(format!("server said {}", r.status)),
                    Err(e) => status.set(e.to_string()),
                }
                sleep(0).await;
            });
        }
    };
    if let Some(list) = initial {
        status.set(format!("{} from the server", list.len()));
        items.set(list);
    } else {
        load();
    }
    let (shown, list) = (status, items);
    div()
        .class("card")
        .child(h2().text("From the server"))
        .child(p().attr("id", "server-status").text_fn(move || shown.get()))
        .child(ul().attr("id", "server-list").child(View::keyed(
            move || list.get(),
            |t| t.id,
            |t| {
                li().class_if("done", move || t.done)
                    .text(t.title)
                    .into_view()
            },
        )))
        .child(button().attr("id", "reload").text("Reload").on_click(load))
        .into_view()
}

/// A button that panics, to see the console message and the development overlay.
#[must_use]
pub fn panic_button() -> View {
    button()
        .attr("id", "boom")
        .text("panic")
        .on_click(|| panic!("boom: this is the panic overlay demo"))
        .into_view()
}

/// All of them. The server renders this with its state; the browser builds the same view from
/// the state embedded in the page (`app`), or fetches it when the page has none.
#[must_use]
pub fn demo(state: Option<AppState>) -> View {
    div()
        .child(h1().text("quench-ui"))
        .child(counter())
        .child(todo())
        .child(server_todos(state.map(|s| s.server_todos)))
        .child(panic_button())
        .into_view()
}

/// The state a page embeds: which page it is, and what it was rendered from.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum PageState {
    Demo(AppState),
    Notes(notes::NotesState),
    /// The routed app under `/app`: the router reads where it is from the URL.
    Site,
    /// The component gallery.
    Gallery,
}

impl PageState {
    /// The JSON the page embeds.
    #[must_use]
    pub fn to_json(&self) -> String {
        quench_serde::json::to_string(self)
    }
}

/// What the browser starts: the page the server rendered, from the state it embedded. With no
/// state (a page that was not rendered on the server) it is the demo, fetching what it needs.
#[must_use]
pub fn app() -> View {
    let state = quench_ui_web::initial_state()
        .and_then(|json| quench_serde::json::from_str::<PageState>(&json).ok());
    match state {
        Some(PageState::Notes(notes)) => notes::notes_view(notes),
        Some(PageState::Demo(demo_state)) => demo(Some(demo_state)),
        Some(PageState::Gallery) => gallery::gallery(),
        Some(PageState::Site) => {
            let router = quench_ui::router::Router::new(std::rc::Rc::new(
                quench_ui_web::BrowserHistory::new(),
            ));
            site::site(&router)
        }
        None => demo(None),
    }
}

#[cfg(feature = "counter")]
quench_ui_web::app!(counter);
#[cfg(all(feature = "todo", not(feature = "counter")))]
quench_ui_web::app!(todo);
#[cfg(not(any(feature = "counter", feature = "todo")))]
quench_ui_web::app!(app);
