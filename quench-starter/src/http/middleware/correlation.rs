//! Ported from `crate::actix::middleware::correlation`. Same behavior:
//! propagate an inbound `x-correlation-id` or mint one, echo it on the
//! response, and record it on the current tracing span.
//!
//! Known gap vs. the actix version: it also stashed the id in
//! `HttpMessage::extensions()` so a handler could read it back
//! (`get_correlation_id`). `quench_http::Request` has no typed
//! per-request extension map yet, so that accessor isn't ported - add one
//! if/when a handler actually needs to read its own correlation id rather
//! than just having it flow through the tracing span.

use async_trait::async_trait;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};
use uuid::Uuid;

const CORRELATION_ID_HEADER: &str = "x-correlation-id";

pub struct CorrelationId;

#[async_trait]
impl Middleware for CorrelationId {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let correlation_id = req
            .header(CORRELATION_ID_HEADER)
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        tracing::Span::current().record("correlation_id", &correlation_id);

        next.call(req)
            .await
            .header("x-correlation-id", &correlation_id)
    }
}

/// Helper to inject correlation ID into outgoing HTTP requests.
pub fn inject_correlation_id_header(
    builder: reqwest::RequestBuilder,
    correlation_id: &str,
) -> reqwest::RequestBuilder {
    builder.header(CORRELATION_ID_HEADER, correlation_id)
}
