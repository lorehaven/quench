// `auth`/`jwks`/`jwt`/`realm`/`session`/`signing` never touched actix-web -
// they live in `crate::domain` now, shared with `crate::http::domain`, and
// are re-exported below unchanged so `quench_auth::actix::domain::X` keeps
// resolving. `sso_client` is still actix-specific (it builds
// `HttpResponse`s) and stays a real module here.
pub use crate::domain::auth;
pub use crate::domain::jwks;
pub use crate::domain::jwt;
pub use crate::domain::realm;
pub use crate::domain::session;
pub use crate::domain::signing;
pub mod sso_client;
