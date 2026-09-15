//! Quench's own HTTP framework: a lightweight (poem-shaped, not
//! actix-shaped) core, routing that keeps actix-router's regex-constrained
//! wildcard path segments (needed for the docker registry / cargo sparse
//! index APIs), annotation-driven route and component discovery
//! (`#[get]`/`#[post]`/... and `#[injectable]`, via `inventory` - no
//! per-service `.service(...)` registration list to maintain), and a real
//! dependency-injection container that resolves the whole component graph
//! at startup (topological order, missing-dependency and cycle detection)
//! rather than threading `Arc`s through every scope by hand.
//!
//! Routes/components defined in a *library* crate only get discovered if
//! the final binary actually links that crate's code in - see
//! [`route::RouteRegistration`]'s doc comment for what that means in
//! practice and how to avoid silently losing routes to it.

// Re-exported so `#[get]`/`#[post]`/`#[injectable]`-generated code only
// needs `quench-http` itself as a dependency, not each of these directly.
pub use async_trait;
pub use http;
pub use inventory;

pub mod body;
pub mod di;
pub mod endpoint;
pub mod error;
pub mod extract;
pub mod middleware;
pub mod multipart;
pub mod request;
pub mod response;
pub mod route;
pub mod router;
pub mod server;

pub mod prelude {
    pub use crate::di::{Container, ContainerBuilder};
    pub use crate::endpoint::{Endpoint, IntoResponse};
    pub use crate::error::{DiError, ExtractError, HttpError};
    pub use crate::extract::{
        BodyLimit, Bytes, Extension, Form, FromRequest, Inject, Json, Limited, Path, Query,
    };
    pub use crate::http;
    pub use crate::middleware::{Middleware, OnPathPrefix, wrap};
    pub use crate::multipart::{Field, Multipart};
    pub use crate::request::Request;
    pub use crate::response::Response;
    pub use crate::route::{RouteRegistration, discover_routes};
    pub use crate::router::{Router, RouterBuilder, mount};
    pub use crate::server::{
        DEFAULT_GRACEFUL_TIMEOUT, load_tls, serve_http, serve_http_on, serve_https, serve_https_on,
    };
    pub use quench_http_macros::{delete, get, injectable, patch, post, put};
}
