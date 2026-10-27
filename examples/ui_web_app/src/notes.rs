//! A small notes app that shows the whole fullstack path.
//!
//! The page is rendered on the server for whoever is signed in, the browser takes it over, and
//! every change is a call to a `#[server]` function. Written once, built for the server and for
//! the browser.

use quench_fullstack::{ServerError, client, server};
use quench_serde::{Deserialize, Serialize};
use quench_ui::prelude::*;
use quench_ui_web::spawn;

/// Where the (gatehouse-compatible) sign-in endpoints are.
pub const AUTH_BASE: &str = "/api/v1/auth";

/// A note, as the signed-in user sees it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Note {
    pub id: u32,
    pub text: String,
    pub done: bool,
}

/// What the server renders the page from, and the browser starts from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct NotesState {
    pub user: Option<String>,
    pub notes: Vec<Note>,
}

/// The longest note, in characters.
pub const MAX_LEN: usize = 200;

/// Where notes live: in memory, on the server only.
#[cfg(not(target_arch = "wasm32"))]
pub mod store {
    use super::Note;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::sync::broadcast;

    static NOTES: Mutex<Vec<(String, Note)>> = Mutex::new(Vec::new());
    static NEXT: AtomicU32 = AtomicU32::new(1);
    static CHANGES: std::sync::OnceLock<broadcast::Sender<String>> = std::sync::OnceLock::new();

    fn changes() -> &'static broadcast::Sender<String> {
        CHANGES.get_or_init(|| broadcast::channel(64).0)
    }

    /// Everyone who is told when a note changes (the server-sent events stream).
    #[must_use]
    pub fn subscribe() -> broadcast::Receiver<String> {
        changes().subscribe()
    }

    #[must_use]
    pub fn notes_of(owner: &str) -> Vec<Note> {
        let notes = NOTES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        notes
            .iter()
            .filter(|(o, _)| o == owner)
            .map(|(_, n)| n.clone())
            .collect()
    }

    pub fn add(owner: &str, text: &str) -> Note {
        let note = Note {
            id: NEXT.fetch_add(1, Ordering::SeqCst),
            text: text.to_string(),
            done: false,
        };
        NOTES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((owner.to_string(), note.clone()));
        let _ = changes().send(format!("{owner} added note {}", note.id));
        note
    }

    pub fn set_done(owner: &str, id: u32, done: bool) -> Option<Note> {
        let mut notes = NOTES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (_, note) = notes.iter_mut().find(|(o, n)| o == owner && n.id == id)?;
        note.done = done;
        let note = note.clone();
        drop(notes);
        let _ = changes().send(format!(
            "{owner} marked note {id} {}",
            if done { "done" } else { "open" }
        ));
        Some(note)
    }
}

/// The signed-in user's notes.
#[server(auth)]
pub async fn list_notes() -> Result<Vec<Note>, ServerError> {
    Ok(store::notes_of(&quench_fullstack::user()?.sub))
}

/// Adds a note. Empty and over-long text is refused, naming the field.
#[server(auth)]
pub async fn create_note(text: String) -> Result<Note, ServerError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ServerError::invalid("text", "write something"));
    }
    if text.chars().count() > MAX_LEN {
        return Err(ServerError::invalid("text", "too long"));
    }
    Ok(store::add(&quench_fullstack::user()?.sub, text))
}

/// Marks a note done or open. Someone else's note is "not found", not "forbidden".
#[server(auth)]
pub async fn set_done(id: u32, done: bool) -> Result<Note, ServerError> {
    store::set_done(&quench_fullstack::user()?.sub, id, done)
        .ok_or_else(|| ServerError::not_found("no such note"))
}

fn sign_in_form(
    user: &Signal<Option<String>>,
    notes: &Signal<Vec<Note>>,
    message: &Signal<String>,
) -> View {
    let username = create_signal(String::new());
    let password = create_signal(String::new());
    let (name_value, pass_value) = (username.clone(), password.clone());
    let (user, notes, message) = (user.clone(), notes.clone(), message.clone());
    let (name_input, pass_input) = (username.clone(), password.clone());
    form()
        .attr("id", "login-form")
        .on_submit(move || {
            let (name, pass) = (username.get_untracked(), password.get_untracked());
            let (user, notes, message) = (user.clone(), notes.clone(), message.clone());
            spawn(async move {
                match client::sign_in(AUTH_BASE, &name, &pass).await {
                    Err(e) => message.set(e.message().to_string()),
                    Ok(()) => match list_notes().await {
                        Ok(list) => {
                            notes.set(list);
                            user.set(Some(name));
                            message.set(String::new());
                        }
                        Err(e) => message.set(e.to_string()),
                    },
                }
            });
        })
        .child(
            input()
                .attr("id", "username")
                .attr("placeholder", "user name")
                .attr_fn("value", move || name_value.get())
                .on_input(move |v| name_input.set(v.to_string())),
        )
        .child(
            input()
                .attr("id", "password")
                .attr("type", "password")
                .attr("placeholder", "password")
                .attr_fn("value", move || pass_value.get())
                .on_input(move |v| pass_input.set(v.to_string())),
        )
        .child(button().attr("type", "submit").text("Sign in"))
        .into_view()
}

fn note_row(note: Note, notes: &Signal<Vec<Note>>, message: &Signal<String>) -> View {
    let (notes, message) = (notes.clone(), message.clone());
    let (id, done) = (note.id, note.done);
    li().class_if("done", move || done)
        .child(span().text(note.text))
        .child(
            button()
                .class("toggle")
                .text(if done { "reopen" } else { "done" })
                .on_click(move || {
                    let (notes, message) = (notes.clone(), message.clone());
                    spawn(async move {
                        match set_done(id, !done).await {
                            Ok(updated) => notes.update(|list| {
                                if let Some(n) = list.iter_mut().find(|n| n.id == id) {
                                    *n = updated;
                                }
                            }),
                            Err(e) => message.set(e.message().to_string()),
                        }
                    });
                }),
        )
        .into_view()
}

fn notes_list(
    user: &Signal<Option<String>>,
    notes: &Signal<Vec<Note>>,
    message: &Signal<String>,
) -> View {
    let draft = create_signal(String::new());
    let (draft_value, draft_input) = (draft.clone(), draft.clone());
    let (add_notes, add_message) = (notes.clone(), message.clone());
    let (out_user, out_notes, out_message) = (user.clone(), notes.clone(), message.clone());
    let (list, row_notes, row_message) = (notes.clone(), notes.clone(), message.clone());
    let who = user.get_untracked().unwrap_or_default();
    let log = create_signal(Vec::<String>::new());
    // Changes made anywhere (another tab, another device) arrive over a server-sent events stream.
    // It is the browser's alone: a server render has no use for it, and the view that opened it
    // closes it when it goes away (signing out rebuilds this region).
    #[cfg(target_arch = "wasm32")]
    {
        use quench_ui_web::{EventSource, SseEvent};
        let source = std::rc::Rc::new(EventSource::open("/api/notes/stream", &["note"]));
        let (reader, feed) = (source.clone(), log.clone());
        spawn(async move {
            while let Some(event) = reader.next().await {
                if let SseEvent::Message { data, .. } = event {
                    feed.update(|log| log.push(data));
                }
            }
        });
        on_cleanup(move || source.close());
    }
    div()
        .child(p().attr("id", "who").text(format!("Signed in as {who}")))
        .child(
            button()
                .attr("id", "signout")
                .text("Sign out")
                .on_click(move || {
                    let (user, notes, message) =
                        (out_user.clone(), out_notes.clone(), out_message.clone());
                    spawn(async move {
                        let _ = client::sign_out(AUTH_BASE).await;
                        user.set(None);
                        notes.set(Vec::new());
                        message.set(String::new());
                    });
                }),
        )
        .child(
            form()
                .attr("id", "note-form")
                .on_submit(move || {
                    let text = draft.get_untracked();
                    let (notes, message, draft) =
                        (add_notes.clone(), add_message.clone(), draft.clone());
                    spawn(async move {
                        match create_note(text).await {
                            Ok(note) => {
                                notes.update(|list| list.push(note));
                                message.set(String::new());
                                draft.set(String::new());
                            }
                            Err(e) => message.set(e.message().to_string()),
                        }
                    });
                })
                .child(
                    input()
                        .attr("id", "note-text")
                        .attr("placeholder", "a note")
                        .attr_fn("value", move || draft_value.get())
                        .on_input(move |v| draft_input.set(v.to_string())),
                )
                .child(button().attr("type", "submit").text("Add")),
        )
        .child(p().text("Activity"))
        .child(ul().attr("id", "activity").child(View::keyed(
            move || log.get().into_iter().enumerate().collect::<Vec<_>>(),
            |(i, _)| *i,
            |(_, line)| li().text(line).into_view(),
        )))
        .child(ul().attr("id", "notes").child(View::keyed(
            move || list.get(),
            |n| format!("{}:{}", n.id, n.done),
            move |n| note_row(n, &row_notes, &row_message),
        )))
        .into_view()
}

/// The page: a sign-in form, or the user's notes. The same function renders it on the server (for
/// whoever the request is from) and in the browser.
#[must_use]
pub fn notes_view(init: NotesState) -> View {
    let user = create_signal(init.user);
    let notes = create_signal(init.notes);
    let message = create_signal(String::new());
    // If the server says the session is over (it expired, or ended elsewhere), show the form.
    #[cfg(target_arch = "wasm32")]
    {
        let user = user.clone();
        client::on_unauthorized(move || user.set(None));
    }
    let shown = message.clone();
    let signed_in = user.clone();
    div()
        .class("card")
        .child(h2().text("Notes"))
        .child(
            p().attr("id", "message")
                .class("error")
                .text_fn(move || shown.get()),
        )
        .child(View::dynamic(move || {
            if signed_in.get().is_some() {
                notes_list(&user, &notes, &message)
            } else {
                sign_in_form(&user, &notes, &message)
            }
        }))
        .into_view()
}
