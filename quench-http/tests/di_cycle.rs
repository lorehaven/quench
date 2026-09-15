//! Two components that depend on each other are caught as a cycle at
//! startup rather than deadlocking or blowing the stack.

use quench_http::di::{ContainerBuilder, DiError};
use quench_http::prelude::injectable;
use std::sync::Arc;

struct A {
    #[allow(dead_code)]
    b: Arc<B>,
}

struct B {
    #[allow(dead_code)]
    a: Arc<A>,
}

#[injectable]
async fn provide_a(b: Arc<B>) -> A {
    A { b }
}

#[injectable]
async fn provide_b(a: Arc<A>) -> B {
    B { a }
}

#[tokio::test]
async fn mutual_dependency_is_reported_as_a_cycle() {
    let err = ContainerBuilder::new().build().await.unwrap_err();
    match err {
        DiError::Cycle(stuck) => {
            assert_eq!(
                stuck.len(),
                2,
                "both A and B should be reported stuck: {stuck:?}"
            );
            assert!(stuck.iter().any(|n| n.contains("::A")), "{stuck:?}");
            assert!(stuck.iter().any(|n| n.contains("::B")), "{stuck:?}");
        }
        other => panic!("expected Cycle, got {other:?}"),
    }
}
