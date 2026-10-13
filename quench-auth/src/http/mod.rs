//! The quench-http side of auth. The framework-agnostic parts (JWT,
//! sessions, realm cookies/URLs, Ed25519 signing) live in `crate::domain`;
//! `middleware::{Auth, RequireWrite}`, `routers::ui::{is_ui_authenticated,
//! get_user_from_req}`, and `domain::sso_client`'s request/response-touching
//! functions are built on `quench_http` in this module.
//!
//! `SessionDb` reaches auth code through the DI container
//! (`req.container().get::<SessionDb>()`), not a parameter threaded through
//! every function - a service wires it in once with `ContainerBuilder::provide_arc(session_db)`.

pub mod domain;
pub mod middleware;
pub mod routers;
