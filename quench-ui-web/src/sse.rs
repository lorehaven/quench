//! Server-sent events, through the browser's `EventSource`.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::future::poll_fn;
use std::task::{Poll, Waker};

use crate::host;

/// What an [`EventSource`] reports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SseEvent {
    /// The connection is open.
    Open,
    /// An event arrived. `name` is `message` when the server sent none.
    Message { name: String, data: String },
    /// The connection was lost and the browser is reconnecting by itself.
    Reconnecting,
    /// The connection failed for good, or was closed. Nothing follows.
    Closed,
}

#[derive(Default)]
struct Source {
    queue: VecDeque<SseEvent>,
    waker: Option<Waker>,
    closed: bool,
}

#[derive(Default)]
struct Sources {
    next: u32,
    open: HashMap<u32, Source>,
}

thread_local! {
    static SOURCES: RefCell<Sources> = RefCell::new(Sources::default());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    let old = SOURCES.with(|s| std::mem::take(&mut *s.borrow_mut()));
    drop(old);
}

/// Called from `q_sse_event`. Events for a source that was dropped are ignored.
pub fn event(id: u32, kind: u32, name: String, data: String) {
    let event = match kind {
        0 => SseEvent::Open,
        1 => SseEvent::Message { name, data },
        2 => SseEvent::Reconnecting,
        3 => SseEvent::Closed,
        _ => return,
    };
    let waker = SOURCES.with(|s| {
        let mut s = s.borrow_mut();
        let source = s.open.get_mut(&id)?;
        if source.closed {
            return None;
        }
        source.closed = event == SseEvent::Closed;
        source.queue.push_back(event);
        source.waker.take()
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// A server-sent events stream. Dropping it closes it.
///
/// ```ignore
/// let source = EventSource::open("/api/notes/stream", &["note"]);
/// spawn(async move {
///     while let Some(event) = source.next().await {
///         if let SseEvent::Message { data, .. } = event { /* ... */ }
///     }
/// });
/// ```
#[derive(Debug)]
pub struct EventSource {
    id: u32,
}

impl EventSource {
    /// Connects to `url` (same origin, with the page's cookies) and listens for the unnamed event
    /// and for each event name in `names`; the browser cannot report names it was not told.
    #[must_use]
    pub fn open(url: &str, names: &[&str]) -> Self {
        let id = SOURCES.with(|s| {
            let mut s = s.borrow_mut();
            let id = s.next;
            s.next = s.next.wrapping_add(1);
            s.open.insert(id, Source::default());
            id
        });
        let names: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| !n.contains('\n'))
            .collect();
        host::sse_open(id, url, &names.join("\n"));
        Self { id }
    }

    /// The next thing that happened, waiting if nothing has. `None` once the stream has failed
    /// for good or been closed, after the events already queued.
    pub async fn next(&self) -> Option<SseEvent> {
        poll_fn(|cx| {
            SOURCES.with(|s| {
                let mut s = s.borrow_mut();
                let Some(source) = s.open.get_mut(&self.id) else {
                    return Poll::Ready(None);
                };
                if let Some(event) = source.queue.pop_front() {
                    return Poll::Ready(Some(event));
                }
                if source.closed {
                    return Poll::Ready(None);
                }
                source.waker = Some(cx.waker().clone());
                Poll::Pending
            })
        })
        .await
    }

    /// Closes the stream. Events already queued can still be read; then `next` gives `None`.
    pub fn close(&self) {
        let waker = SOURCES
            .try_with(|s| {
                let mut s = s.borrow_mut();
                let source = s.open.get_mut(&self.id)?;
                if source.closed {
                    return None;
                }
                source.closed = true;
                source.waker.take()
            })
            .ok()
            .flatten();
        host::sse_close(self.id);
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl Drop for EventSource {
    fn drop(&mut self) {
        self.close();
        let _ = SOURCES.try_with(|s| s.borrow_mut().open.remove(&self.id));
    }
}
