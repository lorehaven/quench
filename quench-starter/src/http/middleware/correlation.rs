//! Propagate an inbound `x-correlation-id` or mint one, echo it on the
//! response, and record it on the current tracing span.
//!
//! The id is not stashed in the request's extensions, so a handler can't
//! read it back - add that if a handler ever needs its own correlation id
//! rather than just having it flow through the tracing span.

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
    builder: quench_client::transport::RequestBuilder,
    correlation_id: &str,
) -> quench_client::transport::RequestBuilder {
    builder.header(CORRELATION_ID_HEADER, correlation_id)
}
