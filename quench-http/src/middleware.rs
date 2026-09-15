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

/// Applies `inner` only to requests whose path starts with `prefix`,
/// passing everything else straight through untouched.
///
/// `wrap` applies to the *whole* app it's given - correct when a
/// middleware really does need to see every request (the correlation-id
/// middleware, say), but plenty don't: an `Auth` wrap belongs on a
/// service's `/api/*` scope, not on its `/health` probe or its own
/// `/login` route. `web::scope("/api/v1/things").wrap(Auth::new(...))` is
/// what this replaces - actix-web scopes each carry their own middleware
/// stack; quench-http's discovered routes are all one flat router, so
/// scoping a middleware to part of it is a middleware concern instead of a
/// routing one.
pub struct OnPathPrefix<M> {
    prefix: &'static str,
    inner: M,
}

impl<M> OnPathPrefix<M> {
    pub fn new(prefix: &'static str, inner: M) -> Self {
        Self { prefix, inner }
    }
}

#[async_trait]
impl<M: Middleware> Middleware for OnPathPrefix<M> {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        if req.uri().path().starts_with(self.prefix) {
            self.inner.handle(req, next).await
        } else {
            next.call(req).await
        }
    }
}
