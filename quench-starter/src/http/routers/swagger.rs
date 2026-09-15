use crate::common::routes::with_base_path;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::{Response, get};

#[get("/swagger-ui")]
async fn swagger_redirect() -> Response {
    Response::new(StatusCode::PERMANENT_REDIRECT).header("Location", with_base_path("/swagger-ui/"))
}

#[get("/swagger-ui/")]
async fn swagger_index_redirect() -> Response {
    Response::new(StatusCode::PERMANENT_REDIRECT)
        .header("Location", with_base_path("/swagger-ui/index.html"))
}

/// Referencing these by name (as opposed to only through the `#[get]`
/// macro's `inventory::submit!`) is what guarantees this module's object
/// code - and with it, the `#[used]` statics the routes are registered
/// through - actually gets linked into a binary that calls
/// `crate::http::serve`. `quench-starter` is a dependency, so its `.rlib`
/// is linked lazily: without a real reference like this one, a build
/// where nothing else happens to pull this module in would silently lose
/// these two routes. See [`quench_http::route::RouteRegistration`]'s doc
/// comment for the full explanation.
pub(crate) fn force_link() {
    let _ = swagger_redirect as fn() -> _;
    let _ = swagger_index_redirect as fn() -> _;
}
