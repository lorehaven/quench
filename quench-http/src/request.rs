use crate::body::InboundBody;
use crate::di::Container;
use quench_proto::{Extensions, HeaderMap, Method, Uri};
use std::sync::Arc;

/// An incoming HTTP request.
///
/// Holds headers/method/uri, captured path parameters
/// (filled in by the router - see [`crate::router::Router`]), the streamed
/// body, a handle to the application's dependency container so extractors
/// can pull injected *services* out of it, and a per-request extension map
/// for *data* a middleware wants a handler to see (decoded auth claims, a
/// correlation id, ...) - the `Inject`/`Extension` split, for the
/// reason that a service is one shared instance the container resolves
/// once, a claims struct is fresh per request and has nowhere else to live.
#[derive(Debug)]
pub struct Request {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    pub(crate) params: Vec<(String, String)>,
    /// Bytes of the URI path already consumed by [`crate::router::mount`].
    prefix_skip: usize,
    body: Option<InboundBody>,
    container: Arc<Container>,
    extensions: Extensions,
}

impl Request {
    #[must_use]
    pub fn new(
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: InboundBody,
        container: Arc<Container>,
    ) -> Self {
        Self {
            method,
            uri,
            headers,
            params: Vec::new(),
            prefix_skip: 0,
            body: Some(body),
            container,
            extensions: Extensions::new(),
        }
    }

    #[must_use]
    pub const fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    pub const fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.method
    }

    #[must_use]
    pub const fn uri(&self) -> &Uri {
        &self.uri
    }

    #[must_use]
    pub const fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub const fn container(&self) -> &Arc<Container> {
        &self.container
    }

    /// Takes the body out for consumption by an extractor. Only one
    /// extractor per request may consume the body.
    pub fn take_body(&mut self) -> InboundBody {
        self.body.take().unwrap_or_else(InboundBody::empty)
    }

    /// Skips the first `n` bytes of the path when the router matches it -
    /// what mounting a sub-app at a runtime-configured prefix (see
    /// [`crate::router::mount`]) needs under the hood.
    pub const fn skip_prefix(&mut self, n: usize) {
        self.prefix_skip += n;
    }

    /// The part of the URI path the router matches against.
    pub(crate) fn routed_path(&self) -> &str {
        &self.uri.path()[self.prefix_skip..]
    }
}
