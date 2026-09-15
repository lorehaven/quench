//! Ported from `crate::actix::middleware::logger::FilteredLogger`. The
//! actix version needed a `Transform`/`Service` pair (~60 lines) to get an
//! around-the-request hook; `quench_http::Middleware` is the same shape as
//! a `tower`/`poem` middleware - one `async fn`.

use async_trait::async_trait;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

#[derive(Clone)]
pub struct FilteredLogger {
    skip_prefixes: Vec<String>,
}

impl Default for FilteredLogger {
    fn default() -> Self {
        let skip = envmnt::get_or("LOG_SKIP_PREFIXES", "");
        let skip_prefixes = skip
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self { skip_prefixes }
    }
}

#[async_trait]
impl Middleware for FilteredLogger {
    async fn handle(&self, req: Request, next: &dyn Endpoint) -> Response {
        let path = req.uri().path().to_string();
        let method = req.method().to_string();

        let response = next.call(req).await;
        let status = response.status();

        let should_log = !status.is_success()
            || !self
                .skip_prefixes
                .iter()
                .any(|prefix| path.starts_with(prefix.as_str()));
        if should_log {
            tracing::info!("{method} {path} -> {status}");
        }

        response
    }
}
