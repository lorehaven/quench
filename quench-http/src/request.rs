use crate::body::InboundBody;
use crate::di::Container;
use actix_router::Path as RoutedPath;
use bytestring::ByteString;
use http::{Extensions, HeaderMap, Method, Uri};
use std::sync::Arc;

/// An incoming HTTP request: headers/method/uri, captured path parameters
/// (filled in by the router - see [`crate::router::Router`]), the streamed
/// body, a handle to the application's dependency container so extractors
/// can pull injected *services* out of it, and a per-request extension map
/// for *data* a middleware wants a handler to see (decoded auth claims, a
/// correlation id, ...) - the same `Inject`/`Extension` split axum and
/// actix-web (`web::Data` vs `HttpMessage::extensions()`) both make, for the
/// same reason: a service is one shared instance the container resolves
/// once, a claims struct is fresh per request and has nowhere else to live.
pub struct Request {
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    pub(crate) params: RoutedPath<ByteString>,
    body: Option<InboundBody>,
    container: Arc<Container>,
    extensions: Extensions,
}

impl Request {
    pub fn new(
        method: Method,
        uri: Uri,
        headers: HeaderMap,
        body: InboundBody,
        container: Arc<Container>,
    ) -> Self {
        let path = ByteString::from(uri.path().to_owned());
        Self {
            method,
            uri,
            headers,
            params: RoutedPath::new(path),
            body: Some(body),
            container,
            extensions: Extensions::new(),
        }
    }

    pub fn extensions(&self) -> &Extensions {
        &self.extensions
    }

    pub fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn uri(&self) -> &Uri {
        &self.uri
    }

    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.params.get(name)
    }

    pub fn container(&self) -> &Arc<Container> {
        &self.container
    }

    /// Takes the body out for consumption by an extractor. Only one
    /// extractor per request may consume the body.
    pub fn take_body(&mut self) -> InboundBody {
        self.body.take().unwrap_or_else(InboundBody::empty)
    }

    /// Skips the first `n` characters of the path when the router matches
    /// it - what mounting a sub-app at a runtime-configured prefix (see
    /// [`crate::router::mount`]) needs under the hood, mirroring
    /// actix-web's own scope nesting.
    pub fn skip_prefix(&mut self, n: u16) {
        self.params.skip(n);
    }
}
