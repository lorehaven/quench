//! A dependency doesn't have to be `#[injectable]` - anything seeded via
//! `ContainerBuilder::provide` (config read from env, a DB pool built by
//! `async fn init()`, ...) satisfies the graph just as well.

use quench_http::prelude::injectable;
use std::sync::Arc;

struct ExternallyConstructed;

struct NeedsProvidedValue {
    #[allow(dead_code)]
    dep: Arc<ExternallyConstructed>,
}

#[injectable]
async fn provide_needs_provided_value(dep: Arc<ExternallyConstructed>) -> NeedsProvidedValue {
    NeedsProvidedValue { dep }
}

#[tokio::test]
async fn provided_value_satisfies_a_dependency_without_being_injectable() {
    let container = quench_http::di::ContainerBuilder::new()
        .provide(ExternallyConstructed)
        .build()
        .await
        .expect("ExternallyConstructed was provided directly");

    container
        .get::<NeedsProvidedValue>()
        .expect("resolved via the provided ExternallyConstructed");
}
