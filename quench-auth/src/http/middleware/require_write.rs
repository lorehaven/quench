//! Blanket write check: any non-GET/HEAD/OPTIONS request needs the
//! `"write"` action. It reads the `Claims` `Auth` sets, so `Auth` has to wrap
//! `RequireWrite` - see `crate::http::middleware`'s doc comment for the exact
//! call.

use crate::domain::jwt::{Claims, JwtConfig};
use async_trait::async_trait;
use quench_http::prelude::http::{Method, StatusCode};
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

pub struct RequireWrite {
    config: JwtConfig,
}

impl RequireWrite {
    pub fn new(config: JwtConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Middleware for RequireWrite {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let bypass = !self.config.auth_enabled || is_safe(req.method());
        if bypass {
            return next.call(req).await;
        }

        let allowed = req
            .extensions()
            .get::<Claims>()
            .is_some_and(|claims| claims.can(&self.config.service_name, "write"));

        if allowed {
            next.call(req).await
        } else {
            tracing::warn!(
                "RequireWrite: {} {} refused - no write permission on {}",
                req.method(),
                req.uri().path(),
                self.config.service_name
            );
            Response::new(StatusCode::FORBIDDEN)
        }
    }
}

/// Methods that never need `write`, whatever the route behind them does.
fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}
