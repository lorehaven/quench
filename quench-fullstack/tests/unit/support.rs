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
