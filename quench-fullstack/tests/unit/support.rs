//! Helpers: turn a response into something to assert on.

use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::prelude::{Request, Response};
use quench_proto::{HeaderMap, Method, Uri};
use std::sync::Arc;

pub struct Out {
    pub status: u16,
    headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Out {
    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

pub async fn out(response: Response) -> Out {
    let inner = response.into_inner();
    let body = inner
        .body
        .collect_limited(64 << 20)
        .await
        .expect("body")
        .to_vec();
    Out {
        status: inner.status.as_u16(),
        headers: inner.headers,
        body,
    }
}

pub async fn request(path: &str, headers: &[(&str, &str)]) -> Request {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.insert(*name, quench_proto::HeaderValue::from_str(value).unwrap());
    }
    Request::new(
        Method::GET,
        path.parse::<Uri>().unwrap(),
        map,
        InboundBody::empty(),
        Arc::new(ContainerBuilder::new().build().await.unwrap()),
    )
}

/// A compressible blob and a small one, standing in for a wasm module and an icon.
pub fn big() -> Vec<u8> {
    b"the quick brown fox jumps over the lazy dog. ".repeat(100)
}

// ---- running the browser's calls against a server in the same process ---------------------------

use quench_http::di::Container;
use quench_http::prelude::Endpoint;
use quench_proto::HeaderValue;
use quench_ui_web::mock;
use std::cell::RefCell;
use std::future::Future;
use std::rc::Rc;

/// A container holding a `JwtConfig`, as a service that verifies sessions has.
pub async fn container_with(jwt: quench_auth::domain::jwt::JwtConfig) -> Arc<Container> {
    Arc::new(ContainerBuilder::new().provide(jwt).build().await.unwrap())
}

fn parse_headers(text: &str) -> HeaderMap {
    let mut map = HeaderMap::new();
    for line in text.lines() {
        if let Some((name, value)) = line.split_once(':')
            && let Ok(value) = HeaderValue::from_str(value.trim())
        {
            map.append(name.trim(), value);
        }
    }
    map
}

/// What the browser would add on its own to a same-origin request.
pub const SAME_ORIGIN: &[(&str, &str)] = &[
    ("host", "app.test"),
    ("origin", "https://app.test"),
    ("sec-fetch-site", "same-origin"),
];

/// Answers every fetch the client has started with the endpoint, as the network would, adding
/// `browser` headers to each. Returns how many were answered.
pub async fn pump(
    endpoint: &dyn Endpoint,
    container: &Arc<Container>,
    browser: &[(&str, &str)],
) -> usize {
    let mut served = 0;
    for fetch in mock::fetches() {
        let mut headers = parse_headers(&fetch.headers);
        for (name, value) in browser {
            headers.insert(*name, HeaderValue::from_str(value).unwrap());
        }
        let request = Request::new(
            fetch.method.parse().unwrap(),
            fetch.url.parse::<Uri>().unwrap(),
            headers,
            InboundBody::from_bytes(fetch.body.clone()),
            container.clone(),
        );
        let response = endpoint.call(request).await.into_inner();
        let body = response
            .body
            .collect_limited(64 << 20)
            .await
            .unwrap()
            .to_vec();
        let mut text = String::new();
        for (name, value) in &response.headers {
            text.push_str(&format!(
                "{}: {}\n",
                name.as_str(),
                value.to_str().unwrap_or("")
            ));
        }
        mock::complete_fetch(fetch.id, u32::from(response.status.as_u16()), &body, &text);
        served += 1;
    }
    served
}

/// Runs `future` in the browser's executor, answering its fetches with `endpoint` until it
/// finishes.
pub async fn drive<T: 'static>(
    endpoint: &dyn Endpoint,
    container: &Arc<Container>,
    browser: &[(&str, &str)],
    future: impl Future<Output = T> + 'static,
) -> T {
    let slot: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let out = slot.clone();
    quench_ui_web::spawn(async move {
        *out.borrow_mut() = Some(future.await);
    });
    for _ in 0..200 {
        mock::run_ticks();
        if slot.borrow().is_some() {
            break;
        }
        if pump(endpoint, container, browser).await == 0 {
            // Nothing was in flight: a timer may be what it waits for.
            if let Some((id, _)) = mock::timers().first().copied() {
                mock::fire_timer(id);
            }
        }
    }
    mock::run_ticks();
    slot.borrow_mut().take().expect("the call did not finish")
}

/// A `POST` as a browser would send a server function call, plus any `headers` given.
pub fn post(
    path: &str,
    headers: &[(impl AsRef<str>, impl AsRef<str>)],
    body: &[u8],
    container: &Arc<Container>,
) -> Request {
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        map.insert(
            name.as_ref(),
            HeaderValue::from_str(value.as_ref()).unwrap(),
        );
    }
    Request::new(
        Method::POST,
        path.parse::<Uri>().unwrap(),
        map,
        InboundBody::from_bytes(body.to_vec()),
        container.clone(),
    )
}

/// The headers a well-formed call carries.
pub const RPC: &[(&str, &str)] = &[("content-type", "application/json"), ("x-quench-rpc", "1")];

pub fn with(base: &[(&str, &str)], extra: &[(&str, &str)]) -> Vec<(String, String)> {
    base.iter()
        .chain(extra)
        .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
        .collect()
}
