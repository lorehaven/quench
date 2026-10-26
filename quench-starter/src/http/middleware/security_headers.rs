//! Hardening headers every response of a service carries.
//!
//! `X-Content-Type-Options: nosniff` stops a browser reinterpreting a body as another content type
//! (an uploaded file or an error page turning into script). A handler that sets the header itself
//! keeps its value.

use quench_core::async_trait;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

const NOSNIFF: &str = "x-content-type-options";

#[derive(Debug)]
pub struct SecurityHeaders;

#[async_trait]
impl Middleware for SecurityHeaders {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let response = next.call(req).await;
        if response.headers().contains_key(NOSNIFF) {
            response
        } else {
            response.header(NOSNIFF, "nosniff")
        }
    }
}
