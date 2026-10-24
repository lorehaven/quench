//! Server-sent events (the `text/event-stream` format).

use crate::body::Body;
use crate::message::Response;
use crate::status::StatusCode;
use quench_core::bytes::Bytes;
use quench_core::stream::{Stream, StreamExt};
use std::convert::Infallible;
use std::fmt;

/// One event. Newlines inside `event` and `id` are dropped (they would end the field);
/// newlines inside data become one `data:` line each.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    name: Option<String>,
    data: Vec<String>,
    id: Option<String>,
    retry: Option<u64>,
    comment: Option<String>,
}

fn single_line(text: &str) -> String {
    text.chars().filter(|c| !matches!(c, '\r' | '\n')).collect()
}

impl Event {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An event carrying `data`.
    #[must_use]
    pub fn data(mut self, data: impl AsRef<str>) -> Self {
        self.data = data
            .as_ref()
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .split('\n')
            .map(str::to_string)
            .collect();
        self
    }

    /// The event name (`event:`), which selects the client-side listener.
    #[must_use]
    pub fn event(mut self, name: impl AsRef<str>) -> Self {
        self.name = Some(single_line(name.as_ref()));
        self
    }

    #[must_use]
    pub fn id(mut self, id: impl AsRef<str>) -> Self {
        self.id = Some(single_line(id.as_ref()));
        self
    }

    /// How long, in milliseconds, a client should wait before reconnecting.
    #[must_use]
    pub const fn retry(mut self, millis: u64) -> Self {
        self.retry = Some(millis);
        self
    }

    /// A comment line, ignored by clients; an empty one makes a keep-alive.
    #[must_use]
    pub fn comment(mut self, text: impl AsRef<str>) -> Self {
        self.comment = Some(single_line(text.as_ref()));
        self
    }

    /// The wire form, ending with the blank line that dispatches the event.
    #[must_use]
    pub fn to_bytes(&self) -> Bytes {
        Bytes::from(self.to_string())
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(comment) = &self.comment {
            writeln!(f, ":{}{comment}", if comment.is_empty() { "" } else { " " })?;
        }
        if let Some(name) = &self.name {
            writeln!(f, "event: {name}")?;
        }
        if let Some(id) = &self.id {
            writeln!(f, "id: {id}")?;
        }
        if let Some(retry) = self.retry {
            writeln!(f, "retry: {retry}")?;
        }
        for line in &self.data {
            writeln!(f, "data: {line}")?;
        }
        writeln!(f)
    }
}

/// A `200` response streaming `events`, with the headers proxies need to not buffer it.
pub fn response<S>(events: S) -> Response
where
    S: Stream<Item = Event> + Unpin + Send + Sync + 'static,
{
    Response::new(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(
            events.map(|event| Ok::<_, Infallible>(event.to_bytes())),
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quench_core::stream;

    #[quench_test::test]
    fn formats_fields_in_a_stable_order() {
        let event = Event::new()
            .comment("hi")
            .event("tick")
            .id("7")
            .retry(1500)
            .data("a\nb\r\nc");
        assert_eq!(
            event.to_string(),
            ": hi\nevent: tick\nid: 7\nretry: 1500\ndata: a\ndata: b\ndata: c\n\n"
        );
        assert_eq!(&event.to_bytes()[..], event.to_string().as_bytes());
    }

    #[quench_test::test]
    fn minimal_and_keep_alive_events() {
        assert_eq!(Event::new().data("x").to_string(), "data: x\n\n");
        assert_eq!(Event::new().comment("").to_string(), ":\n\n");
        assert_eq!(
            Event::new().data("lone\rcr").to_string(),
            "data: lone\ndata: cr\n\n"
        );
        assert_eq!(Event::new().data("").to_string(), "data: \n\n");
    }

    #[quench_test::test]
    fn newlines_cannot_escape_single_line_fields() {
        let event = Event::new()
            .event("a\nevent: injected")
            .id("1\r\ndata: x")
            .comment("c\nd");
        assert_eq!(
            event.to_string(),
            ": cd\nevent: aevent: injected\nid: 1data: x\n\n"
        );
    }

    #[quench_test::test]
    async fn responses_stream_events_with_event_stream_headers() {
        let events = stream::iter(vec![
            Event::new().data("one"),
            Event::new().event("e").data("two"),
        ]);
        let response = response(events);
        assert_eq!(response.status, StatusCode::OK);
        assert_eq!(
            response.headers.get("content-type").unwrap(),
            "text/event-stream"
        );
        assert_eq!(response.headers.get("cache-control").unwrap(), "no-cache");
        assert_eq!(response.headers.get("x-accel-buffering").unwrap(), "no");
        assert_eq!(response.body.exact_len(), None);
        let body = response.body.collect_limited(1000).await.unwrap();
        assert_eq!(body, "data: one\n\nevent: e\ndata: two\n\n");
    }
}
