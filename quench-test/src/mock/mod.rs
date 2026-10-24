//! A mock HTTP server: it answers from rules and remembers what it was asked.
//!
//! ```no_run
//! use quench_test::mock::{Mock, MockServer, ResponseTemplate, matchers::{method, path}};
//!
//! # async fn demo() {
//! let server = MockServer::start().await;
//! Mock::given(method("GET"))
//!     .and(path("/health"))
//!     .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
//!     .expect(1)
//!     .mount(&server)
//!     .await;
//! let url = format!("{}/health", server.uri());
//! // ... call `url` with any client ...
//! server.verify().await;
//! # }
//! ```
//!
//! Rules are tried in priority order (lower first, default 5) and, among equal priorities, in the order they
//! were mounted. A request no rule matches gets an empty 404. Dropping the server checks the `expect`ed
//! call counts (unless the test is already panicking).

mod http;
pub mod matchers;

use quench_serde::Value;
use std::fmt;
use std::net::SocketAddr;
use std::ops::{Range, RangeFrom, RangeInclusive};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::TcpListener;

/// A request the server received.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// The path, without the query.
    pub path: String,
    /// The query string without the `?`.
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    pub(crate) fn new(
        method: String,
        target: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Self {
        let (path, query) = match target.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (target, String::new()),
        };
        Self {
            method,
            path,
            query,
            headers,
            body,
        }
    }

    /// The first header with this name (case-insensitive).
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The decoded value of a query parameter.
    #[must_use]
    pub fn query_param(&self, name: &str) -> Option<String> {
        self.query
            .split('&')
            .filter(|p| !p.is_empty())
            .find_map(|pair| {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                (percent_decode(key) == name).then(|| percent_decode(value))
            })
    }

    /// The body as text (invalid UTF-8 replaced).
    #[must_use]
    pub fn body_string(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// The body parsed as JSON, if it is.
    #[must_use]
    pub fn body_json(&self) -> Option<Value> {
        quench_serde::json::from_slice::<Value>(&self.body).ok()
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len()
                && bytes[i + 1..=i + 2].iter().all(u8::is_ascii_hexdigit) =>
            {
                let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).unwrap_or("00");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A condition on a request.
pub trait Match: Send + Sync + 'static {
    fn matches(&self, request: &Request) -> bool;
}

impl<F: Fn(&Request) -> bool + Send + Sync + 'static> Match for F {
    fn matches(&self, request: &Request) -> bool {
        self(request)
    }
}

/// What a mock answers.
#[derive(Debug, Clone)]
pub struct ResponseTemplate {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
    pub(crate) delay: Duration,
}

impl ResponseTemplate {
    #[must_use]
    pub const fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Vec::new(),
            delay: Duration::ZERO,
        }
    }

    #[must_use]
    pub fn insert_header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        let name = name.as_ref();
        self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
        self.headers
            .push((name.to_string(), value.as_ref().to_string()));
        self
    }

    /// Adds a header, keeping those of the same name already set (a repeated header).
    #[must_use]
    pub fn append_header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.headers
            .push((name.as_ref().to_string(), value.as_ref().to_string()));
        self
    }

    /// A text body, as `text/plain`.
    #[must_use]
    pub fn set_body_string(self, body: impl Into<String>) -> Self {
        self.set_body_raw(body.into().into_bytes(), "text/plain")
    }

    /// A JSON body, as `application/json`.
    #[must_use]
    pub fn set_body_json(self, body: impl quench_serde::Serialize) -> Self {
        self.set_body_raw(
            quench_serde::json::to_string(&body).into_bytes(),
            "application/json",
        )
    }

    /// Bytes as `application/octet-stream`.
    #[must_use]
    pub fn set_body_bytes(self, body: impl Into<Vec<u8>>) -> Self {
        self.set_body_raw(body.into(), "application/octet-stream")
    }

    /// Bytes with a content type.
    #[must_use]
    pub fn set_body_raw(mut self, body: impl Into<Vec<u8>>, mime: &str) -> Self {
        self.body = body.into();
        self.insert_header("content-type", mime)
    }

    /// Wait this long before answering.
    #[must_use]
    pub const fn set_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

/// How many times a mock is expected to be called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Times {
    min: u64,
    max: Option<u64>,
}

impl Times {
    fn accepts(&self, n: u64) -> bool {
        n >= self.min && self.max.is_none_or(|m| n <= m)
    }
}

impl fmt::Display for Times {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.max {
            Some(max) if max == self.min => write!(f, "exactly {max}"),
            Some(max) => write!(f, "between {} and {max}", self.min),
            None => write!(f, "at least {}", self.min),
        }
    }
}

impl From<u64> for Times {
    fn from(n: u64) -> Self {
        Self {
            min: n,
            max: Some(n),
        }
    }
}

impl From<Range<u64>> for Times {
    fn from(r: Range<u64>) -> Self {
        Self {
            min: r.start,
            max: Some(r.end.saturating_sub(1)),
        }
    }
}

impl From<RangeInclusive<u64>> for Times {
    fn from(r: RangeInclusive<u64>) -> Self {
        Self {
            min: *r.start(),
            max: Some(*r.end()),
        }
    }
}

impl From<RangeFrom<u64>> for Times {
    fn from(r: RangeFrom<u64>) -> Self {
        Self {
            min: r.start,
            max: None,
        }
    }
}

/// The start of a rule: the conditions, before the response.
pub struct MockBuilder {
    matchers: Vec<Box<dyn Match>>,
}

impl MockBuilder {
    /// Adds a condition; the request must satisfy all of them.
    #[must_use]
    pub fn and(mut self, matcher: impl Match) -> Self {
        self.matchers.push(Box::new(matcher));
        self
    }

    /// Finishes the rule with the response it gives.
    #[must_use]
    pub fn respond_with(self, response: ResponseTemplate) -> Mock {
        Mock {
            matchers: self.matchers,
            response,
            expected: None,
            limit: None,
            priority: 5,
            name: None,
        }
    }
}

/// One rule of a mock server.
pub struct Mock {
    matchers: Vec<Box<dyn Match>>,
    response: ResponseTemplate,
    expected: Option<Times>,
    limit: Option<u64>,
    priority: u8,
    name: Option<String>,
}

impl Mock {
    #[must_use]
    pub fn given(matcher: impl Match) -> MockBuilder {
        MockBuilder {
            matchers: vec![Box::new(matcher)],
        }
    }

    /// How many calls the rule must get by the time the server is verified or dropped.
    #[must_use]
    pub fn expect(mut self, times: impl Into<Times>) -> Self {
        self.expected = Some(times.into());
        self
    }

    /// The rule stops matching after this many calls (later rules take over).
    #[must_use]
    pub const fn up_to_n_times(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }

    /// Lower numbers are tried first; the default is 5.
    #[must_use]
    pub const fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// A name shown in verification failures.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Adds the rule to a server.
    pub async fn mount(self, server: &MockServer) {
        server.register(self).await;
    }
}

struct Rule {
    mock: Mock,
    calls: u64,
}

#[derive(Default)]
struct State {
    rules: Vec<Rule>,
    received: Vec<Request>,
}

fn locked(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A running mock server on a free local port.
pub struct MockServer {
    address: SocketAddr,
    state: Arc<Mutex<State>>,
    /// Stops the serving thread.
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

impl fmt::Debug for MockServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MockServer({})", self.address)
    }
}

impl MockServer {
    /// Starts a server on `127.0.0.1` with a free port.
    ///
    /// # Panics
    ///
    /// When no port can be bound.
    pub async fn start() -> Self {
        // The server lives on a thread (and runtime) of its own: a test that blocks its own runtime while it
        // waits for a spawned process must still get answers.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a mock server port");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let address = listener.local_addr().expect("the bound address");
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        let (shutdown, stop) = tokio::sync::oneshot::channel::<()>();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("a mock server runtime");
            runtime.block_on(async move {
                let listener = TcpListener::from_std(listener).expect("an async listener");
                let accept = async {
                    loop {
                        let Ok((stream, _)) = listener.accept().await else {
                            break;
                        };
                        let state = Arc::clone(&shared);
                        tokio::spawn(async move {
                            let (read, mut write) = stream.into_split();
                            let mut reader = BufReader::new(read);
                            while let Some((request, keep_alive)) =
                                http::read_request(&mut reader, &mut write).await
                            {
                                let response = respond(&state, request);
                                if http::write_response(&mut write, &response, keep_alive)
                                    .await
                                    .is_err()
                                    || !keep_alive
                                {
                                    break;
                                }
                            }
                        });
                    }
                };
                tokio::select! {
                    () = accept => {}
                    _ = stop => {}
                }
            });
        });
        Self {
            address,
            state,
            shutdown: Some(shutdown),
        }
    }

    /// `http://127.0.0.1:<port>`.
    #[must_use]
    pub fn uri(&self) -> String {
        format!("http://{}", self.address)
    }

    #[must_use]
    pub const fn address(&self) -> &SocketAddr {
        &self.address
    }

    /// Adds a rule (see also [`Mock::mount`]).
    pub async fn register(&self, mock: Mock) {
        locked(&self.state).rules.push(Rule { mock, calls: 0 });
    }

    /// Every request received so far, oldest first.
    pub async fn received_requests(&self) -> Option<Vec<Request>> {
        Some(locked(&self.state).received.clone())
    }

    /// Forgets all rules and recorded requests.
    pub async fn reset(&self) {
        let mut state = locked(&self.state);
        state.rules.clear();
        state.received.clear();
    }

    /// Checks every `expect`ed call count.
    ///
    /// # Panics
    ///
    /// When a rule was called a number of times it did not expect.
    pub async fn verify(&self) {
        if let Some(problem) = self.problems() {
            panic!("{problem}");
        }
    }

    fn problems(&self) -> Option<String> {
        let state = locked(&self.state);
        let failures: Vec<String> = state
            .rules
            .iter()
            .filter_map(|rule| {
                let expected = rule.mock.expected.as_ref()?;
                (!expected.accepts(rule.calls)).then(|| {
                    format!(
                        "mock {} expected {expected} call(s) but got {}",
                        rule.mock
                            .name
                            .as_deref()
                            .map_or_else(|| "(unnamed)".to_string(), |n| format!("`{n}`")),
                        rule.calls
                    )
                })
            })
            .collect();
        (!failures.is_empty())
            .then(|| format!("mock server verification failed:\n{}", failures.join("\n")))
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if !std::thread::panicking()
            && let Some(problem) = self.problems()
        {
            panic!("{problem}");
        }
    }
}

fn respond(state: &Mutex<State>, request: Request) -> ResponseTemplate {
    let mut state = locked(state);
    state.received.push(request.clone());
    let mut order: Vec<usize> = (0..state.rules.len()).collect();
    order.sort_by_key(|&i| state.rules[i].mock.priority);
    for i in order {
        let rule = &mut state.rules[i];
        let exhausted = rule.mock.limit.is_some_and(|limit| rule.calls >= limit);
        if !exhausted && rule.mock.matchers.iter().all(|m| m.matches(&request)) {
            rule.calls += 1;
            return rule.mock.response.clone();
        }
    }
    ResponseTemplate::new(404)
}

impl fmt::Debug for MockBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MockBuilder({} matchers)", self.matchers.len())
    }
}

impl fmt::Debug for Mock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mock")
            .field("matchers", &self.matchers.len())
            .field("response", &self.response)
            .field("expected", &self.expected)
            .field("name", &self.name)
            .finish()
    }
}
