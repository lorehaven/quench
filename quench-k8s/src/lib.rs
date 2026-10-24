//! A generic Kubernetes API client.
//!
//! Objects are plain [`quench_serde::Value`]s: no generated types, so any kind (custom resources included)
//! can be read, created, patched, deleted, watched and have its log read. A [`Client`] authenticates from a
//! kubeconfig (bearer token, client certificate, basic) or the pod's service account; an [`Api`] is a
//! resource, optionally within one namespace; [`Discovery`] lists what the cluster serves.
//!
//! ```no_run
//! # async fn demo() -> Result<(), quench_k8s::Error> {
//! use quench_k8s::{ApiResource, Client, ListParams, Object};
//!
//! let client = Client::infer()?;
//! let pods = client.namespaced(&ApiResource::pod(), "default");
//! for pod in pods.list(&ListParams::default().labels("app=web")).await?.items {
//!     println!("{}", Object(&pod).name().unwrap_or("?"));
//! }
//! # Ok(()) }
//! ```

mod client;
mod config;
mod discovery;
mod error;
mod resource;

pub use client::{
    Api, Client, DeleteParams, ListParams, LogParams, ObjectList, Patch, PatchParams, PostParams,
    Propagation, WatchEvent, lines,
};
pub use config::{Auth, Config};
pub use discovery::{ApiGroup, Capabilities, Discovery, Scope};
pub use error::{Error, Result, Status};
pub use resource::{ApiResource, Object, guess_plural};
