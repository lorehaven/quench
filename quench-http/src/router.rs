use crate::endpoint::Endpoint;
use crate::request::Request;
use crate::response::Response;
use actix_router::Router as InnerRouter;
use async_trait::async_trait;
use http::{Method, StatusCode};
use std::collections::HashMap;
use std::sync::Arc;

/// All handlers registered for one path pattern, keyed by method. Kept as a
/// small `Vec` (a handful of verbs per resource, at most) rather than a map.
struct RouteEntry {
    handlers: Vec<(Method, Arc<dyn Endpoint>)>,
}

/// Matches a request path (including the regex-constrained, multi-segment
/// wildcards actix-router supports - `/{name:.+}/blobs/{digest}` and
/// friends - unchanged from the actix-web route strings they're ported
/// from) to a registered handler.
pub struct Router {
    inner: InnerRouter<RouteEntry>,
}

impl Router {
    pub fn builder() -> RouterBuilder {
        RouterBuilder::default()
    }

    /// Dispatches to the matching handler. 404s when no pattern matches the
    /// path at all, 405s when the path matches but not for this method.
    pub async fn dispatch(&self, mut req: Request) -> Response {
        match self.inner.recognize(&mut req.params) {
            Some((entry, _id)) => match entry.handlers.iter().find(|(m, _)| m == req.method()) {
                Some((_, endpoint)) => endpoint.call(req).await,
                None => Response::text(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
            },
            None => Response::not_found(),
        }
    }
}

#[async_trait]
impl Endpoint for Router {
    async fn call(&self, req: Request) -> Response {
        self.dispatch(req).await
    }
}

struct Mount {
    prefix: String,
    inner: Arc<dyn Endpoint>,
}

#[async_trait]
impl Endpoint for Mount {
    async fn call(&self, mut req: Request) -> Response {
        if self.prefix.is_empty() || self.prefix == "/" {
            return self.inner.call(req).await;
        }

        let path = req.uri().path();
        let Some(rest) = path.strip_prefix(self.prefix.as_str()) else {
            return Response::not_found();
        };
        if !(rest.is_empty() || rest.starts_with('/')) {
            // `/base-pathological` shouldn't match a `/base-path` mount.
            return Response::not_found();
        }

        req.skip_prefix(self.prefix.len() as u16);
        self.inner.call(req).await
    }
}

/// Mounts `inner` under `prefix` (e.g. `BASE_PATH` read from the
/// environment at startup) - a request outside the prefix 404s before
/// `inner` ever sees it, and one inside it is dispatched with the prefix
/// stripped, so `inner`'s own routes are written as if it owned the root.
pub fn mount(prefix: impl Into<String>, inner: Arc<dyn Endpoint>) -> Arc<dyn Endpoint> {
    Arc::new(Mount {
        prefix: prefix.into(),
        inner,
    })
}

/// Builds a [`Router`]. Patterns that can overlap (a static route and a
/// trailing wildcard covering the same prefix, as in the crates sparse
/// index's `/config.json` vs `/{path:.*}`) are matched in *registration*
/// order, same as actix-web's `.service()` order - register the specific
/// ones first.
#[derive(Default)]
pub struct RouterBuilder {
    // Path pattern -> index into `routes`, so registering a second method on
    // an already-seen pattern extends its handler list instead of shadowing
    // it (actix-router itself only matches on path, not method).
    index: HashMap<&'static str, usize>,
    routes: Vec<(&'static str, RouteEntry)>,
}

impl RouterBuilder {
    pub fn route(
        &mut self,
        method: Method,
        pattern: &'static str,
        endpoint: Arc<dyn Endpoint>,
    ) -> &mut Self {
        match self.index.get(pattern) {
            Some(&i) => self.routes[i].1.handlers.push((method, endpoint)),
            None => {
                self.index.insert(pattern, self.routes.len());
                self.routes.push((
                    pattern,
                    RouteEntry {
                        handlers: vec![(method, endpoint)],
                    },
                ));
            }
        }
        self
    }

    pub fn finish(self) -> Router {
        let mut builder = InnerRouter::build();
        for (pattern, entry) in self.routes {
            builder.path(pattern, entry);
        }
        Router {
            inner: builder.finish(),
        }
    }
}
