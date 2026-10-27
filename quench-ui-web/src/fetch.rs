//! `fetch` for the wasm side: a request, a future for its response, and aborting.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use crate::host;

/// A request to send with [`Request::send`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    #[must_use]
    pub fn new(method: &str, url: &str) -> Self {
        Self {
            method: method.to_ascii_uppercase(),
            url: url.to_string(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    #[must_use]
    pub fn get(url: &str) -> Self {
        Self::new("GET", url)
    }

    #[must_use]
    pub fn post(url: &str) -> Self {
        Self::new("POST", url)
    }

    /// Adds a header. Names and values with a line break are dropped, since headers cross the
    /// boundary one per line.
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        let clean = |s: &str| !s.contains(['\n', '\r']);
        if clean(name) && clean(value) && !name.contains(':') {
            self.headers.push((name.to_string(), value.to_string()));
        }
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Starts the request now. The returned future yields the response; dropping it aborts the
    /// request.
    #[must_use]
    pub fn send(self) -> Fetch {
        let id = FETCHES.with(|f| {
            let mut f = f.borrow_mut();
            let id = f.next;
            f.next = f.next.wrapping_add(1);
            f.pending.insert(id, Pending::default());
            id
        });
        let headers = self
            .headers
            .iter()
            .map(|(n, v)| format!("{n}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        host::fetch(id, &self.method, &self.url, &headers, &self.body);
        Fetch { id, done: false }
    }
}

/// A response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// Whether the status is 2xx.
    #[must_use]
    pub const fn ok(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// The body as text; invalid UTF-8 becomes `U+FFFD`.
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The first header with this name, compared without regard to case.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// Why a request produced no response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The network failed (offline, refused, blocked by CORS); the browser's message.
    Network(String),
    /// The request was aborted.
    Aborted,
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(message) => write!(f, "network error: {message}"),
            Self::Aborted => f.write_str("request aborted"),
        }
    }
}

impl std::error::Error for FetchError {}

#[derive(Default)]
struct Pending {
    result: Option<Result<Response, FetchError>>,
    waker: Option<Waker>,
    aborted: bool,
}

#[derive(Default)]
struct Fetches {
    next: u32,
    pending: HashMap<u32, Pending>,
}

thread_local! {
    static FETCHES: RefCell<Fetches> = RefCell::new(Fetches::default());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn reset() {
    FETCHES.with(|f| *f.borrow_mut() = Fetches::default());
}

/// The response of a request in flight.
#[derive(Debug)]
pub struct Fetch {
    id: u32,
    done: bool,
}

impl Fetch {
    /// A handle that can abort this request from elsewhere.
    #[must_use]
    pub const fn abort_handle(&self) -> AbortHandle {
        AbortHandle { id: self.id }
    }
}

/// Aborts a request. The future then resolves to [`FetchError::Aborted`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbortHandle {
    id: u32,
}

impl AbortHandle {
    /// Does nothing if the request already finished.
    pub fn abort(&self) {
        let pending = FETCHES.with(|f| {
            f.borrow_mut().pending.get_mut(&self.id).is_some_and(|p| {
                let live = p.result.is_none() && !p.aborted;
                p.aborted = true;
                live
            })
        });
        if pending {
            host::fetch_abort(self.id);
        }
    }
}

impl Future for Fetch {
    type Output = Result<Response, FetchError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let id = self.id;
        let result = FETCHES.with(|f| {
            let mut f = f.borrow_mut();
            let pending = f.pending.get_mut(&id)?;
            if let Some(result) = pending.result.take() {
                f.pending.remove(&id);
                Some(result)
            } else {
                pending.waker = Some(cx.waker().clone());
                None
            }
        });
        result.map_or(Poll::Pending, |result| {
            self.done = true;
            Poll::Ready(result)
        })
    }
}

impl Drop for Fetch {
    fn drop(&mut self) {
        if !self.done {
            let live = FETCHES
                .try_with(|f| f.borrow_mut().pending.remove(&self.id).is_some())
                .unwrap_or(false);
            if live {
                host::fetch_abort(self.id);
            }
        }
    }
}

fn parse_headers(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

/// Called from `q_fetch_done`. A completion for a request nobody waits for any more is ignored.
pub fn completed(id: u32, status: u32, body: Vec<u8>, headers: &str) {
    let waker = FETCHES.with(|f| {
        let mut f = f.borrow_mut();
        let pending = f.pending.get_mut(&id)?;
        pending.result = Some(if status == 0 {
            if pending.aborted {
                Err(FetchError::Aborted)
            } else {
                Err(FetchError::Network(
                    String::from_utf8_lossy(&body).into_owned(),
                ))
            }
        } else {
            Ok(Response {
                status: u16::try_from(status).unwrap_or(u16::MAX),
                headers: parse_headers(headers),
                body,
            })
        });
        pending.waker.take()
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// Requests that have not completed.
#[must_use]
pub fn pending_fetches() -> usize {
    FETCHES.with(|f| f.borrow().pending.len())
}
