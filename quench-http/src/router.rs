use crate::endpoint::Endpoint;
use crate::pattern::Pattern;
use crate::request::Request;
use crate::response::Response;
use async_trait::async_trait;
use http::{Method, StatusCode};
use std::collections::HashMap;
use std::sync::Arc;

/// All handlers registered for one path pattern, keyed by method. Kept as a
/// small `Vec` (a handful of verbs per resource, at most) rather than a map.
struct RouteEntry {
    handlers: Vec<(Method, Arc<dyn Endpoint>)>,
}

impl std::fmt::Debug for RouteEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.handlers.iter().map(|(method, _)| method))
            .finish()
    }
}

/// Matches a request path (including the regex-constrained, multi-segment
/// wildcards - `/{name:.+}/blobs/{digest}` and friends, see
/// [`crate::pattern`]) to a registered handler.
///
/// Patterns are tried in
/// registration order.
#[derive(Debug)]
pub struct Router {
    routes: Vec<(Pattern, RouteEntry)>,
}

impl Router {
    #[must_use]
    pub fn builder() -> RouterBuilder {
        RouterBuilder::default()
    }

    /// Dispatches to the matching handler. 404s when no pattern matches the
    /// path at all, 405s when the path matches but not for this method.
    pub async fn dispatch(&self, mut req: Request) -> Response {
        let matched = self
            .routes
            .iter()
            .find_map(|(pattern, entry)| Some((pattern.matches(req.routed_path())?, entry)));

        match matched {
            Some((params, entry)) => {
                req.params = params;
                match entry.handlers.iter().find(|(m, _)| m == req.method()) {
                    Some((_, endpoint)) => endpoint.call(req).await,
                    None => Response::text(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
                }
            }
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

        req.skip_prefix(self.prefix.len());
        self.inner.call(req).await
    }
}

/// Mounts `inner` under `prefix` (e.g.
///
/// `BASE_PATH` read from the
/// environment at startup) - a request outside the prefix 404s before
/// `inner` ever sees it, and one inside it is dispatched with the prefix
/// stripped, so `inner`'s own routes are written as if it owned the root.
pub fn mount(prefix: impl Into<String>, inner: Arc<dyn Endpoint>) -> Arc<dyn Endpoint> {
    Arc::new(Mount {
        prefix: prefix.into(),
        inner,
    })
}

/// Builds a [`Router`].
///
/// Patterns that can overlap (a static route and a
/// trailing wildcard covering the same prefix, as in the crates sparse
/// index's `/config.json` vs `/{path:.*}`) are matched in *registration*
/// order - register the specific ones first.
#[derive(Debug, Default)]
pub struct RouterBuilder {
    // Path pattern -> index into `routes`, so registering a second method on
    // an already-seen pattern extends its handler list instead of shadowing
    // it (matching is on path first, method second).
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
        if let Some(&i) = self.index.get(pattern) {
            self.routes[i].1.handlers.push((method, endpoint));
        } else {
            self.index.insert(pattern, self.routes.len());
            self.routes.push((
                pattern,
                RouteEntry {
                    handlers: vec![(method, endpoint)],
                },
            ));
        }
        self
    }

    #[must_use]
    pub fn finish(self) -> Router {
        Router {
            routes: self
                .routes
                .into_iter()
                .map(|(pattern, entry)| (Pattern::parse(pattern), entry))
                .collect(),
        }
    }
}
