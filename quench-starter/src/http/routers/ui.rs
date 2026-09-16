//! Ported from `crate::actix::routers::ui`. Every service puts its pages
//! under `/ui`, so the bare server root and the bare base path both land
//! there rather than on a 404 - `/ui` itself then decides between the home
//! page and the login page.
//!
//! `server_root_redirect` (bare `/`) can't go through the same
//! `#[get]`/`discover_routes()` auto-registration as `base_path_redirect`
//! (`""`) and `base_path_slash_redirect` (`"/"`): those two live *inside*
//! `mount(base_path, ...)`, but `server_root_redirect` has to answer
//! *outside* it (a request that doesn't even start with `BASE_PATH`) - and
//! when `BASE_PATH` isn't `/`, that outer `"/"` and the mounted router's
//! inner `"/"` are different requests that happen to share a pattern
//! string. Registering both through one discovered router would have the
//! second shadow the first. `crate::http::serve` wires
//! `server_root_redirect` in directly instead - see it for the composition.
//!
//! `base_path_redirect`/`base_path_slash_redirect` *do* go through the
//! normal `#[get]`/`discover_routes()` path, same as any service's own
//! routes - which means, unlike actix's explicit `.service(...)` call
//! order, there's no *guaranteed* ordering between these two and a route a
//! service registers at the same pattern (a root-mounted `{path:.*}`
//! catch-all can match `""`/`"/"` too). In practice `quench-starter`'s own
//! `inventory` registrations link before a dependent service's, so these
//! win - but that rests on linker behavior, not something enforced here. A
//! service whose own routing needs to win at the literal base path should
//! register more specifically than a bare root wildcard.

use crate::common::routes::with_base_path;
use bytes::Bytes;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Request, Response, get};
use std::fs;
use std::path::{Component, Path as FsPath, PathBuf};

pub use quench_auth::http::routers::ui::is_ui_authenticated;

pub fn ui_path(path: &str) -> String {
    let result = with_base_path(&format!("/ui{path}"));
    tracing::debug!("ui_path({}) = {}", path, result);
    result
}

pub fn ui_asset_path(path: &str) -> String {
    ui_path(&format!("/assets{path}"))
}

pub fn ui_login_redirect() -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path("/login"))
}

/// The same redirect, in the form the caller can actually act on - see the
/// actix version's doc comment for why a plain `302` doesn't work for an
/// htmx-driven request.
pub fn ui_login_redirect_for(request: &Request) -> Response {
    if request.header("hx-request").is_some() {
        return Response::new(StatusCode::OK).header("HX-Redirect", ui_path("/login"));
    }
    ui_login_redirect()
}

fn ui_root_redirect() -> Response {
    Response::new(StatusCode::FOUND).header("Location", ui_path(""))
}

/// Used directly by `crate::http::serve`, not auto-discovered - see this
/// module's own doc comment.
pub async fn server_root_redirect() -> Response {
    ui_root_redirect()
}

#[get("")]
pub async fn base_path_redirect() -> Response {
    ui_root_redirect()
}

#[get("/")]
pub async fn base_path_slash_redirect() -> Response {
    ui_root_redirect()
}

/// See `swagger::register_routes`'s doc comment - same reason.
/// `server_root_redirect` doesn't need to be listed here: `crate::http::serve`
/// already calls it directly (see `RootOrMounted`), which is itself
/// enough of a real reference to keep it linked.
pub(crate) fn register_routes() {
    let _ = base_path_redirect as fn() -> _;
    let _ = base_path_slash_redirect as fn() -> _;
}

pub async fn serve_assets(path: &str, dist_path: &str) -> Response {
    let Some(relative) = sanitize_asset_path(path) else {
        return Response::new(StatusCode::BAD_REQUEST);
    };

    let full_path = FsPath::new(dist_path).join(relative);
    let Ok(body) = fs::read(&full_path) else {
        return Response::new(StatusCode::NOT_FOUND);
    };

    let content_type = content_type_for_path(&full_path);
    Response::from_bytes(StatusCode::OK, Bytes::from(body))
        .header("cache-control", "public, max-age=3600")
        .header("content-type", content_type)
}

fn sanitize_asset_path(raw: &str) -> Option<PathBuf> {
    if raw.is_empty() {
        return None;
    }

    let candidate = FsPath::new(raw);
    let mut clean = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            _ => return None,
        }
    }

    Some(clean)
}

fn content_type_for_path(path: &FsPath) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
