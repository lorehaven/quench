//! Proves the DI container does real dependency-graph resolution: a
//! multi-level chain is constructed bottom-up from a single top-level
//! `get::<Service>()`, in the right order, with each component built
//! exactly once.
//!
//! Each scenario (this file, `di_missing_dependency.rs`,
//! `di_provided_value.rs`, `di_cycle.rs`) lives in its own `tests/*.rs`
//! file deliberately: `inventory`'s registry is global *per test binary*,
//! and cargo compiles every integration test file as a separate binary, so
//! putting unrelated `#[injectable]`s in the same file would have them all
//! show up in the same graph.

use quench_http::di::{Container, ContainerBuilder};
use quench_http::prelude::injectable;
use std::sync::Arc;

#[derive(Debug)]
struct Config {
    label: &'static str,
}

#[injectable]
async fn provide_config() -> Config {
    Config { label: "root" }
}

struct Repository {
    config: Arc<Config>,
}

#[injectable]
async fn provide_repository(config: Arc<Config>) -> Repository {
    Repository { config }
}

struct Service {
    repo: Arc<Repository>,
}

#[injectable]
async fn provide_service(repo: Arc<Repository>) -> Service {
    Service { repo }
}

#[tokio::test]
async fn resolves_a_multi_level_chain_in_dependency_order() {
    let container: Container = ContainerBuilder::new()
        .build()
        .await
        .expect("graph resolves");

    let service = container.get::<Service>().expect("Service was constructed");
    assert_eq!(service.repo.config.label, "root");
}
