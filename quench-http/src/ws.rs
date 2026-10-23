//! WebSocket upgrades for handlers.
//!
//! ```ignore
//! #[get("/socket")]
//! async fn socket(upgrade: WebSocketUpgrade) -> Response {
//!     upgrade.on_upgrade(|mut ws| async move {
//!         while let Ok(Some(message)) = ws.recv().await {
//!             if let Message::Text(text) = message {
//!                 let _ = ws.send(Message::Text(text)).await;
//!             }
//!         }
//!     })
//! }
//! ```

use crate::error::HttpError;
use crate::extract::FromRequest;
use crate::request::Request;
use crate::response::Response;
use quench_core::async_trait;
use quench_proto::StatusCode;
use quench_proto::io::Upgraded;
pub use quench_proto::ws::{CloseFrame, Config, Error, Message, WebSocket};

/// Answers `req` with `101 Switching Protocols` and runs `handler` on the connection once it switched.
///
/// # Errors
///
/// A `400` [`HttpError`] when `req` is not a valid WebSocket upgrade request.
pub fn upgrade<F, Fut>(req: &Request, handler: F) -> Result<Response, HttpError>
where
    F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    upgrade_with(req, Config::default(), handler)
}

/// Like [`upgrade`], with explicit size limits.
///
/// # Errors
///
/// A `400` [`HttpError`] when `req` is not a valid WebSocket upgrade request.
pub fn upgrade_with<F, Fut>(
    req: &Request,
    config: Config,
    handler: F,
) -> Result<Response, HttpError>
where
    F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    quench_proto::ws::accept(req.method(), req.headers(), config, handler)
        .map(Response::from)
        .map_err(|err| HttpError::status(StatusCode::BAD_REQUEST, err.to_string()))
}

/// An extractor that validates a WebSocket upgrade request.
///
/// A `#[get]` handler takes it as a parameter and answers with
/// [`on_upgrade`](Self::on_upgrade); a request that is not a valid upgrade is
/// rejected with `400` before the handler runs.
#[derive(Debug, Clone)]
pub struct WebSocketUpgrade {
    key: String,
    config: Config,
}

impl WebSocketUpgrade {
    /// Sets the size limits for this connection.
    #[must_use]
    pub const fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Answers `101 Switching Protocols` and runs `handler` once the connection switched.
    pub fn on_upgrade<F, Fut>(self, handler: F) -> Response
    where
        F: FnOnce(WebSocket<Upgraded>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        Response::from(quench_proto::ws::switch(&self.key, self.config, handler))
    }
}

#[async_trait]
impl FromRequest for WebSocketUpgrade {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let key = quench_proto::ws::validate_upgrade(req.method(), req.headers())
            .map_err(|err| HttpError::status(StatusCode::BAD_REQUEST, err.to_string()))?;
        Ok(Self {
            key,
            config: Config::default(),
        })
    }
}
