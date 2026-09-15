//! `provide_arc` exists because a value that's already `Arc<T>` (e.g.
//! `SessionDb::init()`, which hands one back directly) must not be
//! `provide`d as-is - that would key it on `TypeId::of::<Arc<T>>()`
//! instead of `TypeId::of::<T>()`, and no `Arc<T>`-shaped dependency would
//! ever find it.

use quench_http::di::ContainerBuilder;
use quench_http::prelude::injectable;
use std::sync::Arc;

struct AlreadyArced {
    #[allow(dead_code)]
    label: &'static str,
}

struct Consumer {
    #[allow(dead_code)]
    dep: Arc<AlreadyArced>,
}

#[injectable]
async fn provide_consumer(dep: Arc<AlreadyArced>) -> Consumer {
    Consumer { dep }
}

#[tokio::test]
async fn provide_arc_is_findable_by_dependents_declaring_arc_t() {
    let already: Arc<AlreadyArced> = Arc::new(AlreadyArced { label: "pre-arced" });

    let container = ContainerBuilder::new()
        .provide_arc(already.clone())
        .build()
        .await
        .expect("resolves");

    let resolved = container
        .get::<AlreadyArced>()
        .expect("found under TypeId::of::<AlreadyArced>()");
    assert!(
        Arc::ptr_eq(&resolved, &already),
        "should be the exact same Arc, not a rewrap"
    );

    container
        .get::<Consumer>()
        .expect("Consumer resolved via the pre-arced dependency");
}
