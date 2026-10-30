---
title: Building a UI with quench-ui
parent: Guides
nav_order: 10
---

A complete small app that uses every part of the UI model: signals and memos, a keyed list, a form with validation, routing with a layout and a parameter, translations with plurals and a language switch. It runs against the in-memory `TestRenderer`, so the same code is both the example and its test. The browser entry point is one more line (see the end).

The code below was compiled and run against the crates of this repository (`cargo run` in a scratch project with `quench-ui` as its only dependency); it ends by printing `ok` after all assertions pass.

## Project

```toml
[package]
name = "tasks"
version = "0.1.0"
edition = "2024"

[dependencies]
quench-ui = "0.1"
```

## The app

```rust
use std::rc::Rc;

use quench_ui::form::{Field, Form, rules};
use quench_ui::i18n::I18n;
use quench_ui::prelude::*;
use quench_ui::router::{MemoryHistory, RouteContext, Router, Routes, layout, link};

const EN: &str = "title = Tasks\nopen = { $n ->\n    [one] { $n } open task\n   *[other] { $n } open tasks\n}\nadd = Add\nname-required = Write something\nnot-found = Nothing here\n";
const PL: &str = "title = Zadania\nopen = { $n ->\n    [one] { $n } otwarte zadanie\n    [few] { $n } otwarte zadania\n   *[many] { $n } otwartych zadań\n}\nadd = Dodaj\nname-required = Napisz coś\nnot-found = Nic tu nie ma\n";

#[derive(Clone)]
struct Task {
    id: u32,
    title: String,
    done: Signal<bool>,
}

#[derive(Clone)]
struct Store {
    tasks: Signal<Vec<Task>>,
    next_id: Signal<u32>,
}

impl Store {
    fn add(&self, title: &str) {
        let id = self.next_id.get_untracked();
        self.next_id.set(id + 1);
        let task = Task { id, title: title.to_string(), done: create_signal(false) };
        self.tasks.update(|t| t.push(task));
    }
    fn open_count(&self) -> usize {
        self.tasks.with(|t| t.iter().filter(|t| !t.done.get()).count())
    }
}

fn shell(i18n: &I18n, inner: View) -> View {
    let i18n = i18n.clone();
    let switch = { let i18n = i18n.clone(); button().text("PL").on_click(move || {
        let next = if i18n.language() == "en" { "pl" } else { "en" };
        i18n.set_language(next);
    }) };
    div()
        .child(header().child(h1().text_fn({ let i18n = i18n.clone(); move || i18n.t("title") }))
            .child(nav().child(link("/").text("Home")).child(link("/about").text("About")).child(switch)))
        .child(element("main").child(inner))
        .into_view()
}

fn list_page(store: &Store, i18n: &I18n) -> View {
    let name = Field::new("title", "").rule(rules::required(&i18n.t("name-required")));
    let form = Form::new().field(&name);
    let add = {
        let (store, saved) = (store.clone(), name.clone());
        form.element(move |_| { store.add(&saved.value_untracked()); saved.reset(""); })
            .child(name.input())
            .child(name.error_view())
            .child(button().attr("type", "submit").text_fn({ let i18n = i18n.clone(); move || i18n.t("add") }))
    };
    let count = { let (store, i18n) = (store.clone(), i18n.clone());
        p().text_fn(move || i18n.tr("open", &[("n", store.open_count().into())])) };
    let list = {
        let store = store.clone();
        View::keyed(
            move || store.tasks.get(),
            |t| t.id,
            |t| {
                let done = t.done.clone();
                li().class_if("done", { let d = t.done.clone(); move || d.get() })
                    .child(link(&format!("/task/{}", t.id)).text(t.title.clone()))
                    .child(button().text("x").on_click(move || done.update(|d| *d = !*d)))
                    .into_view()
            },
        )
    };
    div().child(add).child(count).child(ul().child(list)).into_view()
}

fn task_page(ctx: &RouteContext, store: &Store) -> View {
    let (ctx, store) = (ctx.clone(), store.clone());
    p().text_fn(move || {
        let id: Option<u32> = ctx.param("id").and_then(|v| v.parse().ok());
        store.tasks.with(|t| t.iter().find(|x| Some(x.id) == id).map(|x| format!("Task {}: {}", x.id, x.title)))
            .unwrap_or_else(|| "no such task".into())
    }).into_view()
}

fn app(router: &Router, i18n: &I18n, store: &Store) -> View {
    let (i, s, s2, i2) = (i18n.clone(), store.clone(), store.clone(), i18n.clone());
    Routes::new()
        .layout(layout("", move |_ctx, inner| shell(&i, inner))
            .route("", move |_| list_page(&s, &i2))
            .route("task/:id", move |ctx| task_page(ctx, &s2))
            .route("about", |_| p().text("About").into_view()))
        .not_found({ let i = i18n.clone(); move |_| p().text_fn({ let i = i.clone(); move || i.t("not-found") }).into_view() })
        .view(router)
}

fn main() {
    let renderer = Rc::new(TestRenderer::new());
    let root = renderer.root();
    let ((mounted, router), scope) = create_root(|| {
        let i18n = I18n::new(&[("en", EN), ("pl", PL)], "en").expect("valid FTL");
        let store = Store { tasks: create_signal(Vec::new()), next_id: create_signal(1) };
        let router = Router::new(Rc::new(MemoryHistory::new("/")));
        (mount(&renderer, &root, app(&router, &i18n, &store)), router)
    });
    assert!(renderer.text().contains("0 open tasks"));

    // An empty submit shows the rule's message and does not add a task.
    let form = renderer.find_by_tag("form")[0];
    renderer.fire(form, &Event::Submit);
    assert!(renderer.text().contains("Write something"));

    // Type and submit.
    let input = renderer.find_by_tag("input")[0];
    renderer.fire(input, &Event::Input("Buy milk".into()));
    renderer.fire(form, &Event::Submit);
    assert!(renderer.text().contains("1 open task"));
    assert!(renderer.text().contains("Buy milk"));

    // Switch the language: only the text that read it changes.
    let pl = renderer.find_by_tag("button").into_iter().find(|b| renderer.text_of(*b) == "PL").unwrap();
    renderer.fire(pl, &Event::Click);
    assert!(renderer.text().contains("1 otwarte zadanie"));

    // Follow a link: the layout stays, the page changes.
    let a = renderer.find_by_attr("href", "/task/1")[0];
    renderer.fire(a, &Event::Click);
    assert_eq!(router.location().path, "/task/1");
    assert!(renderer.text().contains("Task 1: Buy milk"));

    mounted.unmount();
    scope.dispose();
    println!("ok");
}
```

## What each part does

* **State.** `Store` holds a `Signal<Vec<Task>>`. Each `Task` carries its own `done: Signal<bool>`, because a keyed list keeps the nodes of an item whose key is still present and does not call `render` again with the new item value. Changing data lives in signals; `open_count` reads them, so the counter text updates when any task is toggled. See [reactive API](../crates/quench-ui/api-reactive.md).
* **Keyed list.** `View::keyed(items, key, render)` reconciles by `t.id`. Adding a task renders one new `<li>`; toggling touches one class attribute. Keys must be unique (duplicates panic). See [view API](../crates/quench-ui/api-view.md).
* **Form.** `Field::new("title", "")` with a `required` rule, `Form::element` builds a `<form novalidate>` that validates every field on submit and calls the handler only when all pass; `field.input()` binds an `<input>` (id/name, value, `aria-invalid`), `error_view()` is the `role="alert"` message. The rule message is a plain string captured when the field is built: here it is translated once with `i18n.t`, so it does not follow later language switches. See [form API](../crates/quench-ui/api-form.md).
* **Routing.** `Routes::new().layout(layout("", ...))` makes a shell that stays mounted while the child route changes; `route("task/:id", ...)` reads `ctx.param("id")` inside a view closure, so moving between `/task/1` and `/task/2` patches text instead of rebuilding the page; `link("/x")` renders an `<a>` that navigates through the router on a plain click. `MemoryHistory` stands in for the browser's history. Build the `Routes` inside the reactive root (here inside `create_root`). See [router API](../crates/quench-ui/api-router.md).
* **Translations.** `I18n::new(&[("en", EN), ("pl", PL)], "en")` parses the Fluent subset; the first language is the fallback. `i18n.tr("open", &[("n", count.into())])` inside a `text_fn` closure re-evaluates when the language or the count changes. The plural selector uses CLDR categories (`one`, `few`, `many`, `other`) with the mandatory default variant `*[...]`. See [i18n API](../crates/quench-ui/api-i18n.md).
* **Testing.** `TestRenderer` has `fire(node, &Event)`, `find_by_tag`, `find_by_attr`, `text` and `html`; events are delivered synchronously and batched. See [render API](../crates/quench-ui/api-render.md).

## Pitfalls seen while writing it

* `main` is also a tag helper in the prelude (`main()`); a program that defines `fn main` should use `element("main")` for the tag.
* `Signal` does not implement `PartialEq`, so a struct holding signals cannot derive `PartialEq`.
* `Field` is cloned into closures; clones share state, so `name.reset("")` in the submit handler resets the bound input too.
* Create `Router`, `I18n`, `Field`, `Form` and signals inside a reactive scope (`create_root` or a component), as they create signals.

## Running in the browser

Replace `main` by an entry that returns the view and register it:

```rust
fn app() -> impl IntoView {
    let i18n = I18n::new(&[("en", EN), ("pl", PL)], "en").expect("valid FTL");
    let store = Store { tasks: create_signal(Vec::new()), next_id: create_signal(1) };
    let router = Router::new(std::rc::Rc::new(quench_ui_web::BrowserHistory::new()));
    app_view(&router, &i18n, &store)   // the `app` function above, renamed
}
quench_ui_web::app!(app);
```

`BrowserHistory::new()` is `quench-ui-web`'s history; `app!` defines the `q_main` export the page's `quench.js` calls. See [How the UI renders](how-the-ui-renders.md) and the `quench-ui-web` section of [Crates](../crates/index.md). *Unverified:* the browser snippet was not built for `wasm32-unknown-unknown` here; `BrowserHistory::new` and `app!` are taken from `quench-ui-web/src/lib.rs` and the example `examples/ui_web_app`.
