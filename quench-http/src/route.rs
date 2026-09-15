use crate::endpoint::Endpoint;
use crate::router::{Router, RouterBuilder};
use http::Method;
use std::sync::Arc;

/// Registered by `#[get]`/`#[post]`/`#[put]`/`#[delete]`/`#[patch]` for
/// every discovered handler - this is what makes routing
/// "discovery through annotations rather than explicit registering": no
/// per-service `.service(handler)` list to keep in sync, every annotated
/// function linked into the binary shows up here automatically.
///
/// "linked into the binary" is doing real work in that sentence. Routes
/// defined in a *library* crate (a shared routers module, the way
/// `quench-starter`'s health/swagger/UI-redirect routes are) only show up
/// here if the final binary actually references *something* from that
/// crate. An `.rlib` is a lazily-linked archive: if nothing in the
/// dependent binary calls anything in it, the linker has no reason to pull
/// in any of its object code - and that object code is exactly what the
/// `#[used]` static behind every `#[get]`/`#[injectable]`'s
/// `inventory::submit!` lives in, so it simply never makes it into the
/// executable. A normal service is safe by accident (it always imports
/// real types like `JwtConfig`/`HealthState` from that same crate), but a
/// minimal test binary that only talks to `quench-http` itself can hit
/// this and see zero routes with no error at all. Reference something real
/// from the crate whose routes you need, or they silently vanish.
pub struct RouteRegistration {
    pub method: Method,
    pub pattern: &'static str,
    pub endpoint: fn() -> Arc<dyn Endpoint>,
}

inventory::collect!(RouteRegistration);

/// Builds a [`Router`] out of every `#[get]`/`#[post]`/... discovered in
/// the linked binary, in link order. When two patterns can match the same
/// path (a static route and a trailing wildcard over the same prefix),
/// link/registration order decides - see [`RouterBuilder`].
pub fn discover_routes() -> Router {
    let mut builder = RouterBuilder::default();
    for route in inventory::iter::<RouteRegistration>() {
        builder.route(route.method.clone(), route.pattern, (route.endpoint)());
    }
    builder.finish()
}
