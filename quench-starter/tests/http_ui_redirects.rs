//! `routers::ui::base_path_redirect`/`base_path_slash_redirect` are
//! `#[get]`-discovered like health/swagger, so this exercises them the same
//! way a real service would end up serving them: through
//! `discover_routes()` mounted under a base path.
//!
//! `inventory`'s registrations only survive linking if this binary
//! actually references something real from the crate that defines them:
//! `quench_starter`'s rlib is an archive, and an archive member the linker
//! never has a reason to pull in - because nothing in this binary calls
//! anything in it - doesn't make it into the final binary, `#[used]`
//! statics inside it included. A real service never hits this (it imports
//! `JwtConfig`, `HealthState`, ...), but a test that only talks to
//! `quench_http` needs a real touch too - see `base_path()` below.

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode, Uri};
use quench_http::body::InboundBody;
use quench_http::di::ContainerBuilder;
use quench_http::prelude::{Endpoint, discover_routes, mount};
use quench_http::request::Request;
use std::sync::Arc;

fn get(uri: &str, container: &Arc<quench_http::di::Container>) -> Request {
    Request::new(
        Method::GET,
        uri.parse::<Uri>().unwrap(),
        HeaderMap::new(),
        InboundBody::from_bytes(Bytes::new()),
        container.clone(),
    )
}

#[tokio::test]
async fn bare_base_path_and_its_trailing_slash_both_redirect() {
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let router: Arc<dyn Endpoint> = Arc::new(discover_routes());
    let base_path = quench_starter::common::routes::normalize_base_path("/svc");
    let app = mount(base_path, router);

    let resp = app.call(get("/svc", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);

    let resp = app.call(get("/svc/", &container)).await;
    assert_eq!(resp.status(), StatusCode::FOUND);
}
