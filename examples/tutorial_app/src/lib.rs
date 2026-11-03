//! The tutorial app: a shared pantry list. One crate, built twice: for the server (which renders
//! the page and answers the `#[server]` functions) and for the browser (which takes the page over).
//! The tutorial is `docs/guides/tutorial.md`.

use quench_fullstack::{FormExt, ServerError, server};
use quench_serde::{Deserialize, Serialize};
use quench_ui::form::{Field, Form, rules};
use quench_ui::prelude::*;
use quench_ui_web::spawn;

/// The longest item name, in characters.
pub const MAX_LEN: usize = 60;

/// One line of the list.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Item {
    pub id: u32,
    pub name: String,
    pub bought: bool,
}

/// Where the list lives: in memory, on the server only.
#[cfg(not(target_arch = "wasm32"))]
pub mod store {
    use super::Item;
    use std::sync::Mutex;

    static ITEMS: Mutex<Vec<Item>> = Mutex::new(Vec::new());

    fn with<T>(f: impl FnOnce(&mut Vec<Item>) -> T) -> T {
        f(&mut ITEMS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    #[must_use]
    pub fn all() -> Vec<Item> {
        with(|items| items.clone())
    }

    #[must_use]
    pub fn add(name: &str) -> Item {
        with(|items| {
            let id = items.iter().map(|i| i.id).max().unwrap_or(0) + 1;
            let item = Item {
                id,
                name: name.to_string(),
                bought: false,
            };
            items.push(item.clone());
            item
        })
    }

    #[must_use]
    pub fn set_bought(id: u32, bought: bool) -> Option<Item> {
        with(|items| {
            let item = items.iter_mut().find(|i| i.id == id)?;
            item.bought = bought;
            Some(item.clone())
        })
    }

    pub fn clear() {
        with(Vec::clear);
    }
}

/// The whole list.
#[server]
pub async fn list_items() -> Result<Vec<Item>, ServerError> {
    Ok(store::all())
}

/// Adds an item. Empty and over-long names are refused, naming the field.
#[server]
pub async fn add_item(name: String) -> Result<Item, ServerError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ServerError::invalid("name", "write something"));
    }
    if name.chars().count() > MAX_LEN {
        return Err(ServerError::invalid("name", "too long"));
    }
    Ok(store::add(name))
}

/// Marks an item bought, or not.
#[server]
pub async fn set_bought(id: u32, bought: bool) -> Result<Item, ServerError> {
    store::set_bought(id, bought).ok_or_else(|| ServerError::not_found("no such item"))
}

fn row(item: Item, items: &Signal<Vec<Item>>, message: &Signal<String>) -> View {
    let (items, message) = (items.clone(), message.clone());
    let (id, bought) = (item.id, item.bought);
    li().class_if("bought", move || bought)
        .child(span().text(item.name))
        .child(
            button()
                .class("toggle")
                .text(if bought { "need again" } else { "got it" })
                .on_click(move || {
                    let (items, message) = (items.clone(), message.clone());
                    spawn(async move {
                        match set_bought(id, !bought).await {
                            Ok(updated) => items.update(|list| {
                                if let Some(i) = list.iter_mut().find(|i| i.id == id) {
                                    *i = updated;
                                }
                            }),
                            Err(e) => message.set(e.message().to_string()),
                        }
                    });
                }),
        )
        .into_view()
}

/// The page. The server renders it from `initial`; the browser starts from the same list.
#[must_use]
pub fn pantry_view(initial: Vec<Item>) -> View {
    let items = create_signal(initial);
    let message = create_signal(String::new());

    let name = Field::new("name", "").rule(rules::required("write something"));
    let form = Form::new().field(&name);
    let add_form = form
        .element({
            let (items, name) = (items.clone(), name.clone());
            move |form| {
                form.set_submitting(true);
                let (form, items, name) = (form.clone(), items.clone(), name.clone());
                let value = name.value_untracked();
                spawn(async move {
                    if let Some(item) = form.finish(add_item(value).await) {
                        items.update(|list| list.push(item));
                        name.reset("");
                    }
                });
            }
        })
        .attr("id", "add-form")
        .child(name.input().attr("placeholder", "milk, bread, ..."))
        .child(name.error_view())
        .child(form.error_view())
        .child(button().attr("type", "submit").text("Add"));

    let (list, row_items, row_message) = (items.clone(), items, message.clone());
    div()
        .class("card")
        .child(h2().text("Pantry"))
        .child(add_form)
        .child(
            p().attr("id", "message")
                .class("error")
                .text_fn(move || message.get()),
        )
        .child(ul().attr("id", "items").child(View::keyed(
            move || list.get(),
            |i| format!("{}:{}", i.id, i.bought),
            move |i| row(i, &row_items, &row_message),
        )))
        .into_view()
}

/// The browser's entry: starts from the list the server embedded in the page.
#[cfg(target_arch = "wasm32")]
fn app() -> View {
    let initial = quench_ui_web::initial_state()
        .and_then(|json| quench_serde::json::from_str::<Vec<Item>>(&json).ok())
        .unwrap_or_default();
    pantry_view(initial)
}

#[cfg(target_arch = "wasm32")]
quench_ui_web::app!(app);
