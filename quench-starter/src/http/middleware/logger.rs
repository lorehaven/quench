//! Request logging that skips noisy paths (health probes, assets).
//! `quench_http::Middleware` is a single `async fn` around the endpoint.

use async_trait::async_trait;
use quench_http::prelude::{Endpoint, Middleware, Request, Response};

#[derive(Debug, Clone)]
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
