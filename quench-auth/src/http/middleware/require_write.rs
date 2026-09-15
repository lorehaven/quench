//! Ported from `crate::actix::middleware::require_write`. Same rule, same
//! mounting-order requirement - see that module's doc comment, which this
//! doesn't repeat. The one behavioral difference: `quench_http::wrap`'s
//! composition order is the opposite of actix's `.wrap()` (outermost first,
//! not last), so here `Auth` wraps `RequireWrite`, not the other way round -
//! see `crate::http::middleware`'s own doc comment for the exact call.

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
