use crate::endpoint::Endpoint;
use crate::request::Request;
use crate::response::Response;
use async_trait::async_trait;
use std::sync::Arc;

/// Wraps one [`Endpoint`] in another - a plain `async fn`, unlike actix's
/// hand-rolled `Service`/`Transform` pair. `next.call(req)` runs the rest of
/// the chain.
#[async_trait]
pub trait Middleware: Send + Sync + 'static {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response;
}

struct Wrapped<M> {
    middleware: M,
    inner: Arc<dyn Endpoint>,
}

#[async_trait]
impl<M: Middleware> Endpoint for Wrapped<M> {
    async fn call(&self, req: Request) -> Response {
        self.middleware.handle(req, self.inner.as_ref()).await
    }
}

/// Wraps `inner` with `middleware`, innermost middleware applied first when
/// chained: `wrap(wrap(inner, a), b)` runs `b` then `a` then `inner`.
pub fn wrap<M: Middleware>(inner: Arc<dyn Endpoint>, middleware: M) -> Arc<dyn Endpoint> {
    Arc::new(Wrapped { middleware, inner })
}
