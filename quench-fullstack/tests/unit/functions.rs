//! The server functions the tests call: the shapes a real app sends and every kind of failure.

use quench_fullstack::{ServerError, User, server, user};
use quench_serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum State {
    Open,
    Done { by: String, at: i64 },
    Archived(u32),
    Pair(i8, String),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[allow(clippy::derive_partial_eq_without_eq)] // `score` is a float
pub struct Note {
    pub id: u64,
    pub text: String,
    pub tags: Vec<String>,
    pub state: State,
    pub owner: Option<String>,
    pub score: f64,
    pub extra: BTreeMap<String, i64>,
    pub flag: bool,
    pub small: u8,
}

/// A note that uses every kind of field with awkward values.
pub fn awkward() -> Note {
    Note {
        id: u64::MAX,
        text: "caf\u{e9} \u{1f600} \"quoted\" <b>&amp;</b> \\ \n\t".to_string(),
        tags: vec![String::new(), "a".to_string(), "\u{2028}".to_string()],
        state: State::Done {
            by: "ola".to_string(),
            at: i64::MIN,
        },
        owner: None,
        score: -0.000_000_1,
        extra: BTreeMap::from([("big".to_string(), i64::MAX), ("neg".to_string(), -1)]),
        flag: true,
        small: 255,
    }
}

#[server]
pub async fn add(a: i64, b: i64) -> Result<i64, ServerError> {
    a.checked_add(b)
        .ok_or_else(|| ServerError::invalid("a", "too large"))
}

#[server]
pub async fn echo(note: Note) -> Result<Note, ServerError> {
    Ok(note)
}

#[server]
pub async fn states(list: Vec<State>) -> Result<Vec<State>, ServerError> {
    Ok(list)
}

#[server]
pub async fn nothing() -> Result<(), ServerError> {
    Ok(())
}

#[server]
pub async fn maybe(value: Option<String>) -> Result<Option<String>, ServerError> {
    Ok(value.map(|v| v.to_uppercase()))
}

#[server(auth)]
pub async fn who() -> Result<User, ServerError> {
    user()
}

#[server(auth)]
pub async fn shout(text: String) -> Result<String, ServerError> {
    Ok(format!("{}: {}", user()?.sub, text.to_uppercase()))
}

#[server(auth = "optional")]
pub async fn maybe_who() -> Result<Option<String>, ServerError> {
    Ok(user().ok().map(|u| u.sub))
}

#[server]
pub async fn forgot_auth() -> Result<String, ServerError> {
    user().map(|u| u.sub)
}

#[server(path = "/api/fail")]
pub async fn fail(kind: String) -> Result<(), ServerError> {
    match kind.as_str() {
        "unauthorized" => Err(ServerError::unauthorized("sign in")),
        "forbidden" => Err(ServerError::forbidden("not yours")),
        "not_found" => Err(ServerError::not_found("no such note")),
        "validation" => Err(ServerError::Validation {
            message: "two things are wrong".to_string(),
            fields: vec![
                quench_fullstack::FieldError {
                    field: "title".into(),
                    message: "too short".into(),
                },
                quench_fullstack::FieldError {
                    field: "due".into(),
                    message: "in the past".into(),
                },
            ],
        }),
        "internal" => Err(ServerError::internal(
            "connection to db-7.internal:5432 refused, password=hunter2",
        )),
        _ => Ok(()),
    }
}
